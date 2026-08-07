//! `STATUS.md` §4.3's registered probe for the unresolved-**value**-name row:
//! *"1,425 lines — 79.5% want `any`; the reachable remnant has never been
//! characterised — split the 1,425 by what the baseline wants"*.
//!
//! # The row
//!
//! `depend.rs` walks each gap line's declaration-dependency chain to a **root**
//! and buckets by the root's kind; for an `Identifier` root it substitutes
//! `identifier_reason`, whose last-but-two arm is `the name does not resolve as
//! a VALUE`. This probe re-walks the same chain with the same steps and keeps
//! only the lines landing in that bucket, then splits them three ways:
//!
//! - **(a) want `any`** — upstream's own answer is an `errorType` and its node
//!   builder prints that as `any` ([ADR-0038]). This port prints `error` on
//!   purpose, so these lines are unreachable however good `resolve_name` gets.
//! - **(b) want a nameable type this port could compute** — a symbol of that
//!   name exists in this program, `get_type_of_symbol` answers something that is
//!   not `errorType`, and the string it renders is the string the baseline
//!   wants. This is the only bucket that is work.
//! - **(c) want something behind a named unported mechanism** — the `@lib`
//!   directive the harness drops (`bd tsr-cug`), `globalThis`, `arguments`,
//!   the `CommonJS` ambients, a name declared nowhere in the program at all, or
//!   a candidate whose own type gaps.
//!
//! # Why the mechanism split is not inherited wholesale from `nameres.rs`
//!
//! `nameres.rs` measured this family at `058b4a9` and its largest own-mechanism
//! bucket was *"declared inside a namespace body (no exports arm in
//! `resolve_name`)"* at 434 lines. **That arm has since been built** — `tsr-56r`
//! at `3b7fa44`, +4,319 lines; `BindResult::resolve_name` (`lib.rs:431`) now
//! consults a namespace's `exports`, citing `nameresolver.go:104`. Reusing that
//! label would have reported a ported mechanism as missing, which is the error
//! `docs/conventions.md` names as *"before sizing an item on 'X is unported',
//! grep for X"*. The positional test is kept; the label is not.
//!
//! # The counterfactual, and its limit
//!
//! Bucket (b) needs *"would this port print the right thing if the name
//! resolved"*, which is a forecast of a string and not a count of a shape — the
//! shape this project's sizing rules ask for (`newgen.rs`). It is computed as
//! `get_type_of_symbol` on the candidate rendered through `type_to_string_at`,
//! which is `types_producer::render`'s body (that function is private).
//!
//! It is only meaningful where **the assertion line is the unresolved name
//! itself** (depth 0). Where the chain ran, the baseline's right-hand side
//! belongs to a member access several steps away and the root's own string is
//! not what the line wants; those lines are reported separately and never enter
//! the exact-match bucket.
//!
//! # Controls, with expected values registered before the run
//!
//! - **C1, construction.** Every classified line's root must still fail
//!   `resolve_name(…, VALUE)` at classification time. **Expect 0** violations.
//!   `novaldecl.rs`'s C1 read 39 of 900 because it reached the symbol by a route
//!   the producer does not take; here the classification *is* the producer's
//!   route, so a non-zero reading is a defect and not a margin.
//! - **C2, arithmetic.** Buckets sum to classified.
//! - **C3, pinned by the upstream construct, not by my arithmetic.** Upstream
//!   answers an unresolved name with `errorType`: `checkIdentifier` calls
//!   `getResolvedSymbol`, and `if symbol == c.unknownSymbol { return
//!   c.errorType }` (`checker.go:11046`). The node builder prints `errorType`
//!   as `any` (ADR-0038). So a depth-0 line whose name is declared
//!   **nowhere in this program** must want `any` — upstream could not resolve it
//!   either. **Expect 0** such lines wanting a real type. The expected value
//!   comes from upstream's code, not from a summary of this probe's partition;
//!   any reading above 0 is a line where upstream's program held a declaration
//!   ours does not, and each one names a program-composition mechanism.
//! - **C4, mirror on the machinery, not on the finding.** The bucket-(b)
//!   counterfactual is run in the same pass over lines that are **already
//!   right** and whose node is a resolving identifier reference. If the
//!   counterfactual is faithful it must reproduce those baselines. **Expect
//!   ≥95% agreement**; a low reading means bucket (b) is measuring the
//!   renderer's disagreement rather than the resolver's.
//! - **C5, cross-instrument.** `STATUS.md` §4.3 publishes this row at **1,425
//!   lines, 79.5% want-any**, measured at `b00738d` and re-confirmed at
//!   `7cecc02`. Six builds have landed since; the row is expected to have
//!   **shrunk**, and the printed reconciliation says by how much.
//!
//! # Measured, and two of the five registrations were contradicted
//!
//! Kept above **as registered**, corrected here rather than edited away
//! (`docs/conventions.md`: *"a rule written in advance and quietly reinterpreted
//! afterwards is worse than none"*). At `df13a69`:
//!
//! - **C3 fired at 31**, not 0. Each is a depth-0 line whose name nothing in
//!   this program declares and whose baseline still wants a real type — so
//!   upstream's program held a declaration ours does not. Named in
//!   `checker-notes-nameres.md` §53.
//! - **C4 fired at 85.2%**, below the 95% floor — and its own diagnosis leg says
//!   **99.6%** of the disagreements are lines where the *producer's* answer
//!   differs from `get_type_of_symbol`. The identifier arm does more than read
//!   the symbol (flow narrowing, the export redirect), so the counterfactual
//!   systematically **under**-counts: bucket (b)'s exact figure is a lower bound
//!   and `CandidateDiffers` is the upper one. Reported, not tuned.
//! - **C5's stated expectation was backwards.** The row was expected to have
//!   shrunk from 1,425 and it reads **7,685** — 5.4× larger — reproducing
//!   `depend.rs`'s cell at HEAD *exactly*, line for line and want-any share for
//!   want-any share. The cell is one `(kind, ending)` pair and this probe's
//!   ending histogram has only one entry, so there is no partition question:
//!   `STATUS.md` §4.3's 1,425 is stale, not mis-scoped.
//!
//! [ADR-0038]: ../../../docs/adr/0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md
//!
//! Run with `cargo run --release -p tsr-conformance --example valgap`.

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Verbatim from `depend.rs`. Chains here are short; the cap exists so a shape
/// nobody anticipated is counted instead of hanging.
const MAX_DEPTH: usize = 16;

// ---------------------------------------------------------------------------
// The walk — verbatim from `examples/depend.rs`, so this probe's population is
// that probe's row and not a lookalike. Any edit here is a divergence and must
// be made in both files.
// ---------------------------------------------------------------------------

/// Does this node's own answer gap?
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

/// One step toward what this node's answer depends on.
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

/// Is this root the `Identifier: the name does not resolve as a VALUE` bucket?
///
/// The two guards are `depend.rs`'s `identifier_reason` in the same order it
/// asks them: a declaration's own name and the `b` of `a.b` are different
/// buckets there and must not be admitted here.
fn is_unresolved_value_name<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> Option<&'a str> {
    let Some(Node::Identifier(identifier)) = map.get(id) else { return None };
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
    {
        return None;
    }
    if binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE).is_some() {
        return None;
    }
    Some(identifier.text)
}

// ---------------------------------------------------------------------------
// The split
// ---------------------------------------------------------------------------

/// What the baseline wants for the gap line, and — where it wants a real type —
/// which mechanism owns it. First match wins; the order is by causal strength.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Bucket {
    /// (a) ADR-0038's ceiling: upstream's own answer is an `errorType`.
    WantAny,
    /// (c) The case sets `@lib` and `apply_test_directives` does not read it,
    /// so the global was never in the program. `bd tsr-cug`.
    LibDropped,
    /// (c) `globalThis`, unported.
    GlobalThis,
    /// (c) `arguments`; upstream carries a dedicated `argumentsSymbol` and
    /// `checkIdentifier` has an arm for it (`checker.go:11049`).
    Arguments,
    /// (c) A `CommonJS` ambient this port does not synthesise.
    CommonJsAmbient,
    /// (c) The identifier carries no text — parser error recovery.
    EmptyName,
    /// (c) No symbol of this name anywhere in this program, yet the baseline
    /// wants a real type: upstream's program held a declaration ours does not.
    /// This is C3's population and it should be empty.
    NowhereInProgram,
    /// (c) A symbol of this name exists but carries **only** `ALIAS` —
    /// `SymbolFlags::VALUE` does not include `ALIAS` (`symbol.rs:106`), so
    /// resolving the reference means resolving the alias, and `resolve_alias`
    /// refuses the import forms **for a naming reason**: an alias prints its own
    /// name, so `import * as ns` would render `typeof <stripped file path>`.
    /// `bd tsr-4jk`, `STATUS.md` §5, `checker-notes-novaldecl.md`.
    CandidateAliasOnly,
    /// (c) A symbol of this name exists but carries only a **type** meaning, so
    /// a `VALUE` lookup cannot answer it and neither can upstream's.
    CandidateTypeOnly,
    /// (b) A candidate exists, types, and would print **exactly** what the
    /// baseline wants. The only bucket that is work.
    CandidateExact,
    /// (b/naming) A candidate exists and types, and would print something else.
    /// Resolving it converts a gap into a *wrong* line unless the printed form
    /// is fixed too — the shape `bd tsr-4jk` refused for import aliases.
    CandidateDiffers,
    /// (c) A candidate exists and its own type gaps: downstream of some other
    /// item, and not this row's.
    CandidateGaps,
    /// The chain ran, so the line's baseline belongs to a member access and the
    /// root's own string cannot be compared against it. Reported, never scored.
    CascadeNotComparable,
}

impl Bucket {
    const fn label(self) -> &'static str {
        match self {
            Self::WantAny => "(a) want `any` — ADR-0038, unreachable",
            Self::LibDropped => "(c) @lib directive dropped by the harness (bd tsr-cug)",
            Self::GlobalThis => "(c) globalThis — unported global",
            Self::Arguments => "(c) arguments — unsynthesised global",
            Self::CommonJsAmbient => "(c) CommonJS ambient (require/module/exports/...)",
            Self::EmptyName => "(c) empty identifier text — parser recovery",
            Self::NowhereInProgram => "(c) declared NOWHERE in this program — C3",
            Self::CandidateAliasOnly => "(c) the only candidate is an ALIAS — bd tsr-4jk, refused",
            Self::CandidateTypeOnly => "(c) the only candidate has a TYPE meaning only",
            Self::CandidateExact => "(b) a candidate types and prints EXACTLY what is wanted",
            Self::CandidateDiffers => "(b) a candidate types and prints something ELSE",
            Self::CandidateGaps => "(c) a candidate exists and its own type gaps (downstream)",
            Self::CascadeNotComparable => "     the chain ran — root string not comparable",
        }
    }
}

/// The mechanism arms that need no program state, split out so
/// [`check_classifier`] can drive them with literals.
fn ambient_bucket(text: &str, lib_dropped: bool) -> Option<Bucket> {
    if text.is_empty() {
        return Some(Bucket::EmptyName);
    }
    if lib_dropped {
        return Some(Bucket::LibDropped);
    }
    if text == "globalThis" {
        return Some(Bucket::GlobalThis);
    }
    if text == "arguments" {
        return Some(Bucket::Arguments);
    }
    if matches!(text, "require" | "module" | "exports" | "__dirname" | "__filename" | "process") {
        return Some(Bucket::CommonJsAmbient);
    }
    None
}

/// Asserted on every run, before anything is measured.
fn check_classifier() {
    assert_eq!(ambient_bucket("", false), Some(Bucket::EmptyName));
    assert_eq!(ambient_bucket("Temporal", true), Some(Bucket::LibDropped));
    assert_eq!(ambient_bucket("globalThis", false), Some(Bucket::GlobalThis));
    assert_eq!(
        ambient_bucket("globalThis", true),
        Some(Bucket::LibDropped),
        "the lib arm is asked first, so every other arm's share is a lower bound"
    );
    assert_eq!(ambient_bucket("arguments", false), Some(Bucket::Arguments));
    assert_eq!(ambient_bucket("q", false), None, "an ordinary name needs the program to classify");
}

/// Where a candidate's declaration sits, relative to the reference.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Where {
    OtherFile,
    NamespaceBody,
    SameFile,
    NoDeclaration,
}

impl Where {
    const fn label(self) -> &'static str {
        match self {
            Self::OtherFile => "declared in another file of the program",
            Self::NamespaceBody => "declared inside a namespace/module body",
            Self::SameFile => "declared in this file, out of scope",
            Self::NoDeclaration => "the candidate symbol has no declaration",
        }
    }
}

/// The file a node belongs to, as the topmost ancestor's id.
fn file_of(nodes: &NodeTable, id: NodeId) -> NodeId {
    let mut current = id;
    while let Some(parent) = nodes.parent(current) {
        current = parent;
    }
    current
}

fn where_declared(nodes: &NodeTable, symbol: &tsr_binder::Symbol<'_>, reference: NodeId) -> Where {
    let Some(&declaration) = symbol.declarations.first() else { return Where::NoDeclaration };
    let mut current = nodes.parent(declaration);
    while let Some(node) = current {
        if nodes.kind(node) == SyntaxKind::ModuleDeclaration {
            return Where::NamespaceBody;
        }
        current = nodes.parent(node);
    }
    if file_of(nodes, declaration) == file_of(nodes, reference) {
        Where::SameFile
    } else {
        Where::OtherFile
    }
}

#[derive(Default)]
struct Report {
    classified: usize,
    want_any: usize,
    buckets: BTreeMap<Bucket, usize>,
    bucket_cases: BTreeMap<Bucket, HashMap<String, usize>>,
    bucket_names: BTreeMap<Bucket, HashMap<String, usize>>,
    /// For the two candidate buckets, where the candidate is declared.
    candidate_where: BTreeMap<(Bucket, Where), usize>,
    /// Head `want` → `would print` pairs for [`Bucket::CandidateDiffers`].
    differs_pairs: BTreeMap<String, usize>,
    depths: BTreeMap<usize, usize>,
    /// `depend.rs`'s second key column, so `STATUS.md`'s cell can be found.
    /// `has_step_arm` lists `Identifier`, so the `NO STEP ARM` relabel cannot
    /// fire on any root this probe keeps and is not replicated.
    endings: BTreeMap<&'static str, usize>,
    endings_any: BTreeMap<&'static str, usize>,
    c1_resolves_after_all: usize,
    c3_nowhere_wants_real: usize,
    c4_mirror_total: usize,
    c4_mirror_agrees: usize,
    /// Of the C4 disagreements, how many are lines where the producer's own
    /// answer differs from `get_type_of_symbol` — i.e. the identifier arm did
    /// something more than read the symbol (flow narrowing, export redirect).
    /// That is the expected cause and it makes bucket (b) a *bound*.
    c4_disagree_producer_differs: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.want_any += other.want_any;
        self.c1_resolves_after_all += other.c1_resolves_after_all;
        self.c3_nowhere_wants_real += other.c3_nowhere_wants_real;
        self.c4_mirror_total += other.c4_mirror_total;
        self.c4_mirror_agrees += other.c4_mirror_agrees;
        self.c4_disagree_producer_differs += other.c4_disagree_producer_differs;
        for (k, n) in &other.buckets {
            *self.buckets.entry(*k).or_default() += n;
        }
        for (k, n) in &other.candidate_where {
            *self.candidate_where.entry(*k).or_default() += n;
        }
        for (k, n) in &other.differs_pairs {
            *self.differs_pairs.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.depths {
            *self.depths.entry(*k).or_default() += n;
        }
        for (k, n) in &other.endings {
            *self.endings.entry(k).or_default() += n;
        }
        for (k, n) in &other.endings_any {
            *self.endings_any.entry(k).or_default() += n;
        }
        for (k, cases) in &other.bucket_cases {
            let mine = self.bucket_cases.entry(*k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        for (k, names) in &other.bucket_names {
            let mine = self.bucket_names.entry(*k).or_default();
            for (name, n) in names {
                *mine.entry(name.clone()).or_default() += n;
            }
        }
    }
}

/// `types_producer::render`'s body, which is private: the string this port would
/// print for `type` if it were the answer at `reference`.
fn render(
    checker: &mut tsr_checker::Checker<'_, '_>,
    reference: NodeId,
    id: tsr_checker::TypeId,
) -> String {
    let error = checker.intrinsics().error;
    checker.type_to_string_at(id, reference).unwrap_or_else(|| checker.type_to_string(error))
}

#[allow(clippy::too_many_lines)]
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
    // `apply_test_directives` (`crates/tsr-conformance/src/trace_case.rs:327`)
    // reads `target`, `module`, `moduleresolution` and friends and does **not**
    // read `lib`, so a case setting `@lib` was compiled against the default lib
    // set and any global that line would have brought in is absent by
    // construction. `nameres.rs` §3 measured and named this; `bd tsr-cug`.
    let sets_lib = parsed.options.contains_key("lib");

    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();

    // Every name the binder recorded anywhere in this program. `resolve_name`
    // failing while this map holds the name is what separates "the resolver
    // cannot reach it" from "nothing declares it".
    //
    // **Not filtered to `SymbolFlags::VALUE`, and the first cut was.** `VALUE`
    // excludes `ALIAS` (`symbol.rs:106`), so filtering sent every name whose
    // only declaration is an import alias into `NowhereInProgram` — a bucket
    // meaning "upstream cannot resolve it either", which is the opposite of the
    // truth for an alias. The meaning is recorded per candidate instead.
    let mut candidates: HashMap<&str, Vec<SymbolId>> = HashMap::new();
    for (id, symbol) in bound.symbols().iter() {
        candidates.entry(symbol.name).or_default().push(id);
    }

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
            let id = line_ids[position];

            // C4's mirror population, taken in the same pass: lines that are
            // already RIGHT and whose node is a resolving identifier reference.
            // The bucket-(b) counterfactual is run on them and must reproduce
            // the baseline.
            if want.text == got.line() {
                if let Some(Node::Identifier(identifier)) = map.get(id)
                    && nodes
                        .parent(id)
                        .and_then(|p| map.get(p))
                        .is_none_or(|p| p.name_id() != Some(id))
                    && let Some(symbol) =
                        bound.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)
                {
                    let computed = checker.get_type_of_symbol(symbol);
                    if computed != error {
                        report.c4_mirror_total += 1;
                        let printed = render(&mut checker, id, computed);
                        if want.text == format!("{} : {printed}", got.text) {
                            report.c4_mirror_agrees += 1;
                        } else if types_producer::type_id_at_location(
                            &mut checker,
                            bound,
                            nodes,
                            map,
                            id,
                        ) != computed
                        {
                            report.c4_disagree_producer_differs += 1;
                        }
                    }
                }
                continue;
            }
            let Some(wanted) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if got.type_string != "error" {
                continue;
            }

            // The walk, identical to `depend.rs`'s.
            let mut current = id;
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

            let Some(name) = is_unresolved_value_name(bound, nodes, map, current) else { continue };
            report.classified += 1;
            // `depend.rs` keys its table on (kind, ending), and `STATUS.md`
            // §4.3's 1,425 is one of those cells, not the whole kind row. The
            // ending is carried so C5 reconciles cell-for-cell.
            *report.endings.entry(ending).or_default() += 1;
            *report.depths.entry(depth.min(4)).or_default() += 1;
            // C1: the classification route IS the producer's route, so this
            // must not fire.
            if bound.resolve_name(nodes, map, current, name, SymbolFlags::VALUE).is_some() {
                report.c1_resolves_after_all += 1;
            }

            let bucket = if wanted == "any" {
                report.want_any += 1;
                *report.endings_any.entry(ending).or_default() += 1;
                Bucket::WantAny
            } else if let Some(ambient) = ambient_bucket(name, sets_lib) {
                ambient
            } else if let Some(found) = candidates.get(name) {
                // Prefer a value-meaning candidate; the alias and type-only
                // arms below are what is left when there is none.
                let value = found
                    .iter()
                    .copied()
                    .find(|&s| bound.symbols().get(s).flags.intersects(SymbolFlags::VALUE));
                let Some(candidate) = value else {
                    let any_alias = found
                        .iter()
                        .any(|&s| bound.symbols().get(s).flags.intersects(SymbolFlags::ALIAS));
                    let bucket = if any_alias {
                        Bucket::CandidateAliasOnly
                    } else {
                        Bucket::CandidateTypeOnly
                    };
                    *report.buckets.entry(bucket).or_default() += 1;
                    *report
                        .bucket_cases
                        .entry(bucket)
                        .or_default()
                        .entry(case.name.clone())
                        .or_default() += 1;
                    *report
                        .bucket_names
                        .entry(bucket)
                        .or_default()
                        .entry(name.to_owned())
                        .or_default() += 1;
                    continue;
                };
                let computed = checker.get_type_of_symbol(candidate);
                let placement = where_declared(nodes, bound.symbols().get(candidate), current);
                let bucket = if computed == error {
                    Bucket::CandidateGaps
                } else if depth > 0 {
                    // The line's baseline belongs to a member access several
                    // steps out; the root's own string is not what it wants and
                    // an exact-match test on it would be measuring nothing.
                    Bucket::CascadeNotComparable
                } else {
                    let printed = render(&mut checker, current, computed);
                    if printed == wanted {
                        Bucket::CandidateExact
                    } else {
                        *report
                            .differs_pairs
                            .entry(format!("want {wanted}  |  would print {printed}"))
                            .or_default() += 1;
                        Bucket::CandidateDiffers
                    }
                };
                *report.candidate_where.entry((bucket, placement)).or_default() += 1;
                bucket
            } else {
                // C3: upstream answers an unresolved name with `errorType` and
                // prints it `any`, so a depth-0 line whose name nothing in this
                // program declares should be in `WantAny` and never here.
                if depth == 0 {
                    report.c3_nowhere_wants_real += 1;
                }
                Bucket::NowhereInProgram
            };

            *report.buckets.entry(bucket).or_default() += 1;
            *report
                .bucket_cases
                .entry(bucket)
                .or_default()
                .entry(case.name.clone())
                .or_default() += 1;
            *report.bucket_names.entry(bucket).or_default().entry(name.to_owned()).or_default() +=
                1;
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn main() {
    check_classifier();
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

    println!("# valgap — the unresolved-VALUE-name row, split by what the baseline wants\n");
    println!("classified: {}\n", report.classified);

    println!("## The split — every bucket with its top-1 case share\n");
    let mut rows: Vec<_> = report.buckets.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let total: usize = report.buckets.values().sum();
    for (bucket, n) in &rows {
        let empty = HashMap::new();
        let cases = report.bucket_cases.get(bucket).unwrap_or(&empty);
        let (top, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
        let share = **n as f64 / total.max(1) as f64 * 100.0;
        let top1 = *top_n as f64 / (**n).max(1) as f64 * 100.0;
        println!(
            "  {n:>6} {share:>5.1}%  {:>4} cases, top-1 {top1:>5.1}%  {}\n            {:<62} {top}",
            cases.len(),
            bucket.label(),
            "",
        );
    }

    println!("\n## Head names per bucket — top 6 each\n");
    for (bucket, _) in &rows {
        let empty = HashMap::new();
        let names = report.bucket_names.get(bucket).unwrap_or(&empty);
        let mut head: Vec<_> = names.iter().collect();
        head.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        let head: Vec<String> =
            head.into_iter().take(6).map(|(name, n)| format!("{name} {n}")).collect();
        println!("  {:<62} {}", bucket.label(), head.join(", "));
    }

    println!("\n## Where the candidate is declared, for the candidate buckets\n");
    let mut placements: Vec<_> = report.candidate_where.iter().collect();
    placements.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((bucket, placement), n) in placements {
        println!("  {n:>6}  {:<62} {}", bucket.label(), placement.label());
    }

    println!("\n## `a candidate prints something ELSE` — head pairs, top 12\n");
    let mut pairs: Vec<_> = report.differs_pairs.iter().collect();
    pairs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (pair, n) in pairs.into_iter().take(12) {
        println!("  {n:>6}  {}", &pair[..pair.len().min(120)]);
    }

    println!("\n## How far the chain ran — 0 means the line IS the unresolved name\n");
    for (depth, n) in &report.depths {
        let share = *n as f64 / total.max(1) as f64 * 100.0;
        let label = if *depth >= 4 { "4+".to_string() } else { depth.to_string() };
        println!("  {label:>3} steps  {n:>7}  {share:>5.1}%");
    }

    println!("\n## `depend.rs`'s second key column — which cell STATUS.md §4.3 quotes\n");
    let mut endings: Vec<_> = report.endings.iter().collect();
    endings.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (ending, n) in endings {
        let any = report.endings_any.get(*ending).copied().unwrap_or(0);
        let any_share = any as f64 / (*n).max(1) as f64 * 100.0;
        println!("  {n:>6}  want-any {any:>6} ({any_share:>4.1}%)  {ending}");
    }

    println!("\n## Controls");
    println!(
        "  C1 construction: roots that resolve as VALUE after all  {}  (expect 0)",
        report.c1_resolves_after_all
    );
    println!("  C2 arithmetic:   buckets sum {total}, classified {}", report.classified);
    println!(
        "  C3 upstream-pinned: depth-0 lines declared NOWHERE yet wanting a real type  {}  (expect 0)",
        report.c3_nowhere_wants_real
    );
    let mirror = report.c4_mirror_agrees as f64 / report.c4_mirror_total.max(1) as f64 * 100.0;
    println!(
        "  C4 mirror:       counterfactual reproduces {}/{} right lines = {mirror:.1}%  (expect >=95%)",
        report.c4_mirror_agrees, report.c4_mirror_total
    );
    let disagreements = report.c4_mirror_total - report.c4_mirror_agrees;
    let diagnosed =
        report.c4_disagree_producer_differs as f64 / disagreements.max(1) as f64 * 100.0;
    println!(
        "  C4 diagnosis:    of {disagreements} disagreements, {} ({diagnosed:.1}%) are lines where the\n                   PRODUCER's answer differs from get_type_of_symbol — the identifier arm\n                   does more than read the symbol, so bucket (b) is a LOWER bound",
        report.c4_disagree_producer_differs
    );
    let any_share = report.want_any as f64 / report.classified.max(1) as f64 * 100.0;
    println!(
        "  C5 cross-instrument: STATUS.md §4.3 publishes 1,425 lines at 79.5% want-any (b00738d);\n                       this run reads {} at {any_share:.1}%",
        report.classified
    );
}
