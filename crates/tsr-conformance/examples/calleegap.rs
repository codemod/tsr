//! Split the two never-explored gates in the call funnel — `bd tsr-klm`'s
//! residue after `bd tsr-4sa` shipped.
//!
//! `callgate.rs` re-run at `a57a04b` reports, of 7,303 admitted call lines:
//!
//! ```text
//!   1,398  want-any 247 (17.7%)   callee type is not an object type
//!     878  want-any 196 (22.3%)   identifier: symbol types as a non-object
//! ```
//!
//! 2,276 lines and neither row has ever been split. The standing hypothesis is
//! that they are one mechanism — *"the callee does not type as something with
//! signatures"* — seen from two positions. **This probe tests that rather than
//! assuming it**, and the test is a cross-tabulation of the counter gate
//! against a *direct* classification of the blocking node, so a disagreement
//! shows up as a cell rather than as a missing row.
//!
//! # The population is `callgate.rs`'s, verbatim
//!
//! [`blocking_call`] and the admission rule (the line gaps, the blocking call's
//! callee already has a type) are copied character for character from
//! `callgate.rs`. A different reachability rule makes the two populations
//! incomparable, and the whole point of this probe is that its numbers slot
//! into that instrument's table.
//!
//! # The downstream bucket goes FIRST
//!
//! `docs/conventions.md`: *"when a construct gaps whole on any unhandled part,
//! its row is not its arm's size — split by which part is unhandled, and put
//! the 'every part is handled, something downstream gapped' bucket **first**,
//! so it cannot inflate the rest. It was 77% here."* Bucket **B0** is exactly
//! that: the blocking call node *itself* answers a real type, so nothing about
//! the callee is what stops the line, and the gate row was bumped by some other
//! expression inside the same subtree. It is printed first and every share
//! below it is quoted against the remainder.
//!
//! # Why one fresh `Checker` per node, and why it is enough
//!
//! Same reason as `callgate.rs`: `COUNTERS` is a process-global atomic, so the
//! delta pass must be serial, and a shared checker memoises so a node already
//! visited would read an all-zero delta. One checker per node serves both jobs
//! — the counter delta is taken across `check_expression(call)`, and the
//! classification below reads the *memoised* callee type from the same checker,
//! which is by construction the type the gate saw.
//!
//! # Controls, with their expected values, all fixed before the first run
//!
//! - **C1** — verbatim from `callgate.rs`. Every classified line answers
//!   `errorType` today. Expect **0** violations.
//! - **C2** — the buckets sum to the admitted lines in the two gates. Expect
//!   **exact**. (An arithmetic control over a partition cannot see that the
//!   partition is wrong, which is why it is the weakest one here and why C3–C6
//!   exist.)
//! - **C3 — pinned to this port's own counter declaration order, hard 0.**
//!   `bump(&COUNTERS.callee_not_anonymous)` (`calls.rs:737`) fires under
//!   *exactly* the condition that makes `classify_unresolved_callee`
//!   (`calls.rs:400`) fire — both are `!matches!(.., TypeData::Anonymous { .. })`
//!   on the same `callee_type` — and every arm of that classifier is declared
//!   **after** `callee_not_anonymous` in `define_counters!`. `gate_of` takes the
//!   *last* bumped row. So a `CallExpression` blocking node can reach the gate
//!   `callee type is not an object type` only because a `NewExpression`
//!   somewhere in its subtree bumped `new_callee_not_anonymous`
//!   (`calls.rs:325`) — which carries the **same label string** and is declared
//!   last. Expect **0** `CallExpression` nodes in that gate with no
//!   `NewExpression` anywhere below them. If this fires, the two gates are not
//!   the two positions this probe says they are.
//! - **C4 — pinned to the UPSTREAM construct, not to a summary of it.**
//!   `isUntypedFunctionCall` (`checker.go:9935`) is true when `IsTypeAny(funcType)`,
//!   and both call (`checker.go:8529`) and `new` (`checker.go:8593`) then go to
//!   `resolveUntypedCall` (`checker.go:9902`), which returns `anySignature` —
//!   whose return type is `anyType`. So **a callee typing as `any` makes
//!   upstream print `any` for the call**. Expect the `any` bucket's want-any
//!   share to be **100%**. A shortfall does not say the partition is off; it
//!   says this reading of upstream is wrong, which arithmetic cannot see.
//! - **C5 — pinned to the same place.** `getSignaturesOfStructuredType`
//!   (`checker.go:18964`) returns `nil` unless `t.flags&TypeFlagsStructuredType != 0`,
//!   and `getApparentType` (`checker.go:21744`–`21753`) maps every primitive to
//!   its global interface, none of which declares a call or construct signature.
//!   So a non-`any` intrinsic or literal callee is *not callable upstream
//!   either*: upstream reports and answers `errorType`, which the baseline
//!   renders `any`. Expect want-any **≥ 90%** across those buckets. Not 100%,
//!   because a line reached by route 2 of [`blocking_call`] is an identifier two
//!   hops from the call and its own answer need not be the call's.
//! - **C6 — pinned to this port's shipped decline model, hard 0.** A
//!   `Named { members: Some(_) }` callee that passes all seven declines in
//!   `get_signature_of_named_type` (`signatures.rs:303`–`349`) would have been
//!   *answered* by the shipped arm and cannot be in either gate. Expect **0** in
//!   `WOULD RESOLVE`. Non-zero means this probe's re-implementation of that
//!   model has drifted from the shipped one — which is the only thing that makes
//!   the other `Named` sub-buckets readable.
//! - **C7 — hard 0.** Both gates are the `else` arm of a `TypeData::Anonymous`
//!   destructure (`calls.rs:727`, `expressions.rs:790`), so no admitted node's
//!   callee can be `Anonymous`.
//! - **C8 — hard 0.** The admission rule excludes an `errorType` callee.
//!
//! # One stale-prerequisite test, and its prediction is written down first
//!
//! `get_signature_of_named_type` declines when `needs_namespace_qualifier`
//! (`signatures.rs:346`) is true, on the stated premise that *"`TypeData::Named`
//! bakes the symbol's own name"* and so would print `NumberFormat` where
//! `Intl.NumberFormat` is wanted. That premise is a **hypothesis as of this
//! session**: design P added `Checker::type_to_string_at` (`checker.rs:513`) →
//! `qualified_name_at`, and `types_producer.rs:682` renders *every* assertion
//! through it. **Prediction, before the run: the refusal is stale and its
//! bucket's forecast now matches the baseline.** Measured, not asserted — the
//! probe computes `type_to_string_at(return, assertion node)` per line and
//! compares it to the wanted string, which is `docs/conventions.md`'s match test
//! rather than a shape test.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind, TypeElement};
use tsr_binder::SymbolFlags;
use tsr_checker::calls::counters;
use tsr_checker::types::TypeData;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The two gate labels this probe splits. Both are keyed by the *trimmed* label
/// exactly as `callgate.rs` keys its `gates` map.
const GATE_NOT_OBJECT: &str = "callee type is not an object type";
const GATE_IDENTIFIER: &str = "identifier: symbol types as a non-object";

/// The blocking call node for a gap line, by the two routes
/// `checker-notes-callres.md` §3 describes. **Verbatim from `callgate.rs`** —
/// see the module docs for why it may not be improved here.
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

/// One admitted assertion line: where it is, and what the baseline wants.
///
/// Carried per line rather than collapsed to a count because the forecast in
/// the namespace bucket is `type_to_string_at(return, **this** node)` — a
/// context-sensitive name has no meaning without the reference site.
#[derive(Clone)]
struct Line {
    /// The assertion's own node — the reference site for a qualified name.
    node: NodeId,
    /// The right-hand side of the baseline assertion.
    wanted: String,
}

/// Pass A's finding for one case: blocking node -> the lines it blocks.
type Candidates = BTreeMap<NodeId, Vec<Line>>;

fn discover(case: &tsr_conformance::CaseEntry) -> Option<(String, Candidates, usize)> {
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

    let mut candidates: Candidates = BTreeMap::new();
    let mut not_gap = 0;
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
            let callee = match map.get(call) {
                Some(Node::CallExpression(node)) => node.expression,
                Some(Node::NewExpression(node)) => node.expression,
                _ => None,
            };
            let Some(callee) = callee else { continue };
            if checker.check_expression(callee) == error {
                continue;
            }
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();
            candidates.entry(call).or_default().push(Line { node: id, wanted });
        }
    }
    (!candidates.is_empty()).then_some((case.name.clone(), candidates, not_gap))
}

/// Which gate a counter delta names. **Verbatim from `callgate.rs`** — the last
/// bumped row in declaration order is the deepest gate reached.
fn gate_of(before: &counters::Snapshot, after: &counters::Snapshot) -> Option<&'static str> {
    let mut delta: Vec<(&'static str, u64)> = Vec::new();
    for ((label, a), (_, b)) in after.rows().into_iter().zip(before.rows()) {
        if a > b {
            delta.push((label, a - b));
        }
    }
    if delta.is_empty() {
        return None;
    }
    delta.last().map(|(label, _)| label.trim())
}

/// A signature element of the wanted kind, and whether it is generic.
struct Candidate<'a> {
    r#type: Option<tsr_ast::TypeNode<'a>>,
    generic: bool,
}

fn candidate_of(element: TypeElement<'_>, construct: bool) -> Option<Candidate<'_>> {
    match (element, construct) {
        (TypeElement::CallSignatureDeclaration(node), false) => {
            Some(Candidate { r#type: node.r#type, generic: !node.type_parameters.is_empty() })
        }
        (TypeElement::ConstructSignatureDeclaration(node), true) => {
            Some(Candidate { r#type: node.r#type, generic: !node.type_parameters.is_empty() })
        }
        _ => None,
    }
}

/// `needs_namespace_qualifier` (`signatures.rs:366`), re-implemented: any
/// declaration of the answer type's members symbol has a `ModuleDeclaration`
/// ancestor.
fn is_namespace_qualified(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    members: Option<tsr_binder::SymbolId>,
) -> bool {
    let Some(members) = members else { return false };
    binder.symbols().get(members).declarations.iter().any(|&declaration| {
        nodes.ancestors(declaration).any(|node| nodes.kind(node) == SyntaxKind::ModuleDeclaration)
    })
}

/// Where a callee's `any` came from.
///
/// The `any` bucket is a **mixture** and the first run proved it: 249 of its 326
/// lines forecast the baseline exactly and 77 do not, and every miss wants a
/// real type — `void`, `Foo`, `ConcreteA`. Upstream's `resolveUntypedCall`
/// cannot be wrong about those, so the `any` is **this port's**, not upstream's,
/// and `docs/conventions.md`'s rule applies: *"a population identified by the
/// shape of the answer is not thereby attributed to a mechanism"*. This splits
/// it by the declaration the name resolves to, which is what an arm could
/// actually test at the call site.
fn any_origin(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    callee: tsr_ast::Expression<'_>,
) -> &'static str {
    let tsr_ast::Expression::Identifier(identifier) = callee else {
        return "callee is not a bare name";
    };
    let Some(id) = identifier.node_id else { return "callee has no node id" };
    let Some(symbol) = binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)
    else {
        return "the name resolves to no symbol";
    };
    let Some(declaration) = binder.symbols().get(symbol).value_declaration else {
        return "the symbol has no value declaration";
    };
    match map.get(declaration) {
        // The contextual-typing family, arriving through a new door. An
        // unannotated parameter is `any` here and is contextually typed
        // upstream, so its call's answer is upstream's *parameter* type and not
        // `any` — `STATUS.md` §5's standing refusal, 2,082 lines, 86% entangled.
        Some(Node::ParameterDeclaration(node)) if node.r#type.is_none() => {
            "an UNANNOTATED PARAMETER — contextual typing (refused)"
        }
        Some(Node::ParameterDeclaration(_)) => "a parameter with a written annotation",
        Some(Node::VariableDeclaration(node)) if node.r#type.is_none() => {
            "a variable with no annotation"
        }
        Some(Node::VariableDeclaration(_)) => "a variable with a written annotation",
        Some(Node::ClassDeclaration(_) | Node::ClassExpression(_)) => {
            "a CLASS that types as `any` — this port's own defect"
        }
        Some(Node::FunctionDeclaration(_)) => "a function declaration typing as `any`",
        Some(Node::BindingElement(_)) => "a binding element",
        Some(Node::PropertyDeclaration(_) | Node::PropertySignatureDeclaration(_)) => {
            "a property declaration"
        }
        Some(Node::ImportSpecifier(_) | Node::ImportClause(_) | Node::NamespaceImport(_)) => {
            "an import alias"
        }
        _ => "another declaration form",
    }
}

/// What a bucket asks about, and what the probe can say about it.
struct Verdict {
    /// The printed bucket label. The numeric prefix fixes the report order and
    /// puts B0 — the downstream bucket — first, by construction.
    label: String,
    /// The type the arm would answer with, where the only thing stopping it is
    /// a refusal this probe is re-examining. `None` everywhere else.
    forecast: Option<tsr_checker::TypeId>,
}

impl Verdict {
    fn gap(label: impl Into<String>) -> Self {
        Self { label: label.into(), forecast: None }
    }
}

/// Classify one blocking node directly — position first, then *why* the callee
/// carries no signatures, in the order upstream asks the questions.
#[allow(clippy::too_many_lines)]
fn classify<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    call: NodeId,
    answer: tsr_checker::TypeId,
) -> Verdict {
    let error = checker.intrinsics().error;
    // B0, and it is first for the reason `docs/conventions.md` gives: the call
    // itself is handled, so nothing here is this gate's arm to build.
    if answer != error {
        return Verdict::gap("B0 the call ALREADY ANSWERS — the row was bumped elsewhere below");
    }
    let (callee, construct) = match map.get(call) {
        Some(Node::CallExpression(node)) => (node.expression, false),
        Some(Node::NewExpression(node)) => (node.expression, true),
        _ => return Verdict::gap("B9 the blocking node is neither a call nor a `new`"),
    };
    let Some(callee) = callee else { return Verdict::gap("B9 the callee is absent") };
    let side = if construct { "new" } else { "call" };
    let callee_type = checker.check_expression(callee);

    let (members, callee_text) = match &checker.type_of(callee_type).data {
        // C8. The admission rule excludes this; printed rather than skipped.
        TypeData::Intrinsic { name } if *name == "error" => {
            return Verdict::gap("C8 CONTROL (expect 0) the callee types as `error`");
        }
        // C4's bucket. `isUntypedFunctionCall` (`checker.go:9935`) →
        // `resolveUntypedCall` (`checker.go:9902`) → `anySignature`: upstream
        // answers `any` here, on both sides (`checker.go:8529`, `:8593`).
        TypeData::Intrinsic { name } if *name == "any" => {
            let origin = any_origin(binder, nodes, map, callee);
            return Verdict {
                label: format!("B1 {side}: callee is `any` ({origin})"),
                forecast: Some(checker.intrinsics().any),
            };
        }
        // C5's buckets. Not callable upstream either — `getApparentType` maps
        // these to a global interface with no signatures.
        TypeData::Intrinsic { name } => {
            let name = (*name).to_string();
            return Verdict::gap(format!("B2 {side}: the callee types as `{name}` (an intrinsic)"));
        }
        TypeData::StringLiteral(_)
        | TypeData::NumberLiteral(_)
        | TypeData::BigIntLiteral(_)
        | TypeData::BooleanLiteral(_) => {
            return Verdict::gap(format!("B2 {side}: the callee types as a literal"));
        }
        TypeData::Union { .. } => {
            return Verdict::gap(format!(
                "B3 {side}: the callee is a UNION — upstream needs common signatures"
            ));
        }
        TypeData::Intersection { .. } => {
            return Verdict::gap(format!(
                "B3 {side}: the callee is an INTERSECTION — `getApparentTypeOfIntersectionType`"
            ));
        }
        // C7. Both gates are the `else` of an `Anonymous` destructure.
        TypeData::Anonymous { .. } => {
            return Verdict::gap("C7 CONTROL (expect 0) the callee types as `Anonymous`");
        }
        TypeData::Named { members, text } => (*members, text.clone()),
    };

    let Some(members) = members else {
        return Verdict::gap(format!(
            "B4 {side}: `Named` with NO members table — this port never built them"
        ));
    };
    let declarations: Vec<NodeId> =
        binder.symbols().get(members).declarations.iter().copied().collect();

    // A type parameter callee. Upstream reaches its signatures through
    // `getApparentType` (`checker.go:21731`: instantiable -> base constraint)
    // before `getSignaturesOfType` ever runs, so this is a *different*
    // mechanism from the interface walk below and must not share its bucket.
    if declarations.iter().any(|&d| matches!(map.get(d), Some(Node::TypeParameterDeclaration(_)))) {
        return Verdict::gap(format!(
            "B5 {side}: the callee is a TYPE PARAMETER — needs `getApparentType`"
        ));
    }

    let mut elements: Vec<TypeElement<'_>> = Vec::new();
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
        return Verdict::gap(format!(
            "B6 {side}: `Named` whose members symbol has NO interface declaration \
             (a class instance type, an enum, an alias)"
        ));
    }
    let found: Vec<Candidate<'_>> =
        elements.into_iter().filter_map(|e| candidate_of(e, construct)).collect();
    if found.is_empty() {
        return Verdict::gap(format!(
            "B7 {side}: the interface declares no {side} signature member"
        ));
    }
    if found.iter().any(|candidate| candidate.generic) {
        return Verdict::gap(format!("B7 {side}: a GENERIC candidate — inference (inherited)"));
    }
    let mut resolved: Vec<tsr_checker::TypeId> = Vec::new();
    for candidate in &found {
        let Some(annotation) = candidate.r#type else {
            return Verdict::gap(format!("B7 {side}: the return annotation is absent"));
        };
        let id = checker.get_type_from_type_node(annotation);
        if id == error {
            return Verdict::gap(format!("B7 {side}: the return annotation itself gaps"));
        }
        resolved.push(id);
    }
    if resolved.iter().any(|id| *id != resolved[0]) {
        return Verdict::gap(format!(
            "B7 {side}: an overload set whose candidates DISAGREE (inherited)"
        ));
    }
    let first = resolved[0];
    if checker.type_of(first).flags.intersects(tsr_checker::TypeFlags::TYPE_PARAMETER) {
        return Verdict::gap(format!("B7 {side}: the return is a type parameter (inherited)"));
    }
    let answer_members = match &checker.type_of(first).data {
        TypeData::Named { members, .. } => *members,
        _ => None,
    };
    if is_namespace_qualified(binder, nodes, answer_members) {
        // The stale-prerequisite test. The forecast is carried so the caller can
        // run the match test per line against the *reference site*.
        return Verdict {
            label: format!(
                "B8 {side}: REFUSED namespace-qualified (`signatures.rs:346`) — RE-EXAMINE"
            ),
            forecast: Some(first),
        };
    }
    if heritage {
        return Verdict::gap(format!("B7 {side}: the interface has a heritage clause (inherited)"));
    }
    // C6. Nothing left declines, so the shipped arm would have answered and this
    // node could not have reached either gate.
    Verdict {
        label: format!("C6 CONTROL (expect 0) WOULD RESOLVE — callee `{callee_text}`"),
        forecast: Some(first),
    }
}

/// C3's test: does any `NewExpression` live under `call`?
fn subtree_has_new(nodes: &NodeTable, call: NodeId) -> bool {
    (0..u32::try_from(nodes.len()).unwrap_or(u32::MAX))
        .map(NodeId::new)
        .filter(|&id| nodes.kind(id) == SyntaxKind::NewExpression)
        .any(|id| nodes.ancestors(id).any(|a| a == call))
}

/// One bucket's tally.
#[derive(Default)]
struct Bucket {
    lines: usize,
    want_any: usize,
    cases: BTreeMap<String, usize>,
    forecast_matches: usize,
    forecast_misses: usize,
    miss_examples: Vec<String>,
    /// Lines whose assertion node **is** the blocking call — route 1 of
    /// [`blocking_call`]. Split out because route 2 attributes an *identifier*
    /// line to the call that initialises its declaration, and a forecast that
    /// holds for the call need not hold two hops downstream. Reported so the
    /// floor can be quoted own-node only, the rule
    /// `checker-notes-namedcallee.md` §3 set.
    own_node: usize,
    own_node_matches: usize,
    /// Which cases the forecast misses land in — a miss family concentrated in
    /// one case is a defect, not a rate.
    miss_cases: BTreeMap<String, usize>,
}

fn main() {
    counters::enable();
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");

    // Pass A, parallel: the counters are not read here, only the corpus.
    let found: Vec<_> = cases.par_iter().filter_map(discover).collect();
    let by_name: BTreeMap<&str, &Candidates> =
        found.iter().map(|(name, c, _)| (name.as_str(), c)).collect();
    let admitted_lines: usize = found.iter().flat_map(|(_, c, _)| c.values()).map(Vec::len).sum();
    let c1: usize = found.iter().map(|(_, _, not_gap)| not_gap).sum();

    println!("# calleegap — the two unexplored gates of the call funnel\n");
    println!("cases with candidates  : {}", found.len());
    println!("admitted lines (all gates): {admitted_lines}");

    // Pass B, SERIAL: process-global counters make any parallelism here wrong.
    let mut per_gate: BTreeMap<&'static str, BTreeMap<String, Bucket>> = BTreeMap::new();
    let mut gate_lines: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut gate_any: BTreeMap<&'static str, usize> = BTreeMap::new();
    // The cross-tab that answers "one mechanism or two": gate x syntactic side.
    let mut cross: BTreeMap<(&'static str, &'static str), usize> = BTreeMap::new();
    let mut c3_violations = 0usize;
    let mut c3_examples: Vec<String> = Vec::new();

    for case in &cases {
        let Some(candidates) = by_name.get(case.name.as_str()) else { continue };
        let Some(text) = case.expected_types() else { continue };
        let expected = types_baseline::parse(&text);
        let Ok(parsed) = case.load() else { continue };
        let arena = tsr_core::Arena::new();
        let (program, _, _) =
            types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
        let nodes = program.nodes();
        let map = program.node_map();
        let bound = program.binder();
        for (&call, lines) in *candidates {
            let mut checker =
                tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
            let expression = match map.get(call) {
                Some(Node::CallExpression(node)) => Some(tsr_ast::Expression::CallExpression(node)),
                Some(Node::NewExpression(node)) => Some(tsr_ast::Expression::NewExpression(node)),
                _ => None,
            };
            let Some(expression) = expression else { continue };
            let before = counters::snapshot();
            let answer = checker.check_expression(expression);
            let after = counters::snapshot();
            let Some(gate) = gate_of(&before, &after) else { continue };
            if gate != GATE_NOT_OBJECT && gate != GATE_IDENTIFIER {
                continue;
            }

            let side = match map.get(call) {
                Some(Node::NewExpression(_)) => "new",
                _ => "call",
            };
            *cross.entry((gate, side)).or_default() += lines.len();
            if gate == GATE_NOT_OBJECT && side == "call" && !subtree_has_new(nodes, call) {
                c3_violations += lines.len();
                if c3_examples.len() < 5 {
                    c3_examples.push(case.name.clone());
                }
            }

            let verdict = classify(&mut checker, bound, nodes, map, call, answer);
            let bucket =
                per_gate.entry(gate).or_default().entry(verdict.label.clone()).or_default();
            bucket.lines += lines.len();
            *bucket.cases.entry(case.name.clone()).or_default() += lines.len();
            *gate_lines.entry(gate).or_default() += lines.len();
            for line in lines {
                let is_any = line.wanted == "any";
                bucket.want_any += usize::from(is_any);
                *gate_any.entry(gate).or_default() += usize::from(is_any);
                // The match test, per line and at the line's own reference site:
                // a context-sensitive name has no meaning without one.
                if let Some(forecast) = verdict.forecast {
                    let own = line.node == call;
                    bucket.own_node += usize::from(own);
                    let printed = checker
                        .type_to_string_at(forecast, line.node)
                        .unwrap_or_else(|| "<unnameable>".to_string());
                    if printed == line.wanted {
                        bucket.forecast_matches += 1;
                        bucket.own_node_matches += usize::from(own);
                    } else {
                        bucket.forecast_misses += 1;
                        *bucket.miss_cases.entry(case.name.clone()).or_default() += 1;
                        if bucket.miss_examples.len() < 6 {
                            bucket.miss_examples.push(format!(
                                "{}  want `{}`  forecast `{printed}`{}",
                                case.name,
                                line.wanted,
                                if own { "  [own-node]" } else { "  [route 2]" }
                            ));
                        }
                    }
                }
            }
        }
    }

    println!("\n## One mechanism or two — gate x syntactic position\n");
    println!("The test: cross-tabulate the counter gate against the blocking");
    println!("node's own kind. Two gates that are one mechanism seen from two");
    println!("positions would each carry both sides.\n");
    println!("  {:<46} {:>6} {:>6}", "gate", "new", "call");
    for gate in [GATE_NOT_OBJECT, GATE_IDENTIFIER] {
        println!(
            "  {:<46} {:>6} {:>6}",
            gate,
            cross.get(&(gate, "new")).copied().unwrap_or(0),
            cross.get(&(gate, "call")).copied().unwrap_or(0),
        );
    }

    let mut total = 0usize;
    for gate in [GATE_NOT_OBJECT, GATE_IDENTIFIER] {
        let Some(buckets) = per_gate.get(gate) else { continue };
        let lines = gate_lines.get(gate).copied().unwrap_or(0);
        let any = gate_any.get(gate).copied().unwrap_or(0);
        total += lines;
        let share = |n: usize| {
            if lines == 0 { 0.0 } else { 100.0 * n as f64 / lines as f64 }
        };
        println!("\n## GATE `{gate}` — {lines} lines, want-any {any} ({:.1}%)\n", share(any));
        println!("  {:>6} {:>8} {:>7}  {:<7} {}", "lines", "want-any", "top-1", "match", "bucket");
        // BTreeMap over the numeric-prefixed labels: B0 first, by construction.
        for (label, bucket) in buckets {
            let top = bucket.cases.values().max().copied().unwrap_or(0);
            let top_name =
                bucket.cases.iter().max_by_key(|(_, n)| **n).map_or("", |(name, _)| name.as_str());
            let top_share =
                if bucket.lines == 0 { 0.0 } else { 100.0 * top as f64 / bucket.lines as f64 };
            let any_share = if bucket.lines == 0 {
                0.0
            } else {
                100.0 * bucket.want_any as f64 / bucket.lines as f64
            };
            let matched = if bucket.forecast_matches + bucket.forecast_misses == 0 {
                "—".to_string()
            } else {
                format!("{}/{}", bucket.forecast_matches, bucket.forecast_misses)
            };
            println!(
                "  {:>6} {:>7.1}% {:>6.1}%  {matched:<7} {label}",
                bucket.lines, any_share, top_share,
            );
            println!("         {:>22}  top case: {top_name} ({top})", "");
            if bucket.forecast_matches + bucket.forecast_misses > 0 {
                println!(
                    "         {:>22}  own-node {} of which forecast matches {}",
                    "", bucket.own_node, bucket.own_node_matches
                );
                let mut top_misses: Vec<_> = bucket.miss_cases.iter().collect();
                top_misses.sort_by(|a, b| b.1.cmp(a.1));
                let head: Vec<String> =
                    top_misses.iter().take(5).map(|(n, c)| format!("{n} {c}")).collect();
                if !head.is_empty() {
                    println!("         {:>22}  miss cases: {}", "", head.join(" | "));
                }
            }
            for example in &bucket.miss_examples {
                println!("         {:>22}  MISS {example}", "");
            }
        }
    }

    println!("\n## Controls\n");
    println!("  C1  classified-but-not-gap                     {c1:>6}   (expect 0)");
    println!("  C2  the two gates sum to                       {total:>6}   (vs callgate's 2,276)");
    println!(
        "  C3  `call` nodes in `{GATE_NOT_OBJECT}` with no `new` below  {c3_violations:>6}   (expect 0)"
    );
    for example in &c3_examples {
        println!("        C3 example: {example}");
    }
    println!(
        "  C4  want-any share of the `any`-callee bucket   — read it off the table (expect 100%)"
    );
    println!("  C5  want-any share of the intrinsic/literal buckets — expect >= 90%");
    println!("  C6  WOULD RESOLVE bucket                        — expect 0 lines");
    println!("  C7  `Anonymous` callee bucket                   — expect 0 lines");
    println!("  C8  `error` callee bucket                       — expect 0 lines");
}
