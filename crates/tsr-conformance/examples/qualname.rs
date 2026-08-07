//! The fresh counterfactual for **namespace-qualified naming**, the item
//! `STATUS.md` §5 refuses at *"90.7% accurate on target row; counterfactual lost
//! 3,202 lines, regressed 753 cases"*.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example qualname
//! ```
//!
//! # Why this is re-measured rather than inherited
//!
//! Two things changed under that refusal, and `STATUS.md` §5 records both:
//!
//! 1. **Its population was understated.** `typerefgap.rs` (sixth session) put
//!    **4,473** lines of this family on the `TypeReference` root alone — 4,002
//!    of the 4,010 `no further dependency` row plus 471 through the
//!    `dependency types` ending — against the 1,318 the refusal was priced on.
//! 2. **Its named cause was fixed.** `checker-notes-nameres.md` §51 attributed
//!    the 3,202 lost lines to one defect — *"`resolve_name` cannot see a
//!    namespace's exports"* — and closed with an instruction: *"Nobody should
//!    build the chain again until (1) is fixed and the counterfactual re-run."*
//!    `BindResult::resolve_name` gained the `nameresolver.go:104`–`:146` arm at
//!    `3b7fa44` (`bd tsr-56r`). **The returning condition is met.**
//!
//! So this probe computes, in one pass, the three columns
//! `docs/conventions.md` requires of a mechanism that fires on a **position**:
//! CONVERTS, WOULD PRINT WRONG, and AT RISK. No checker code is changed.
//!
//! # The item is two halves, and they are priced separately
//!
//! - **The resolution half.** `Checker::get_type_from_type_reference`
//!   (`crates/tsr-checker/src/declared.rs`) returns `errorType` for a qualified
//!   name whose leftmost identifier resolves as a namespace, deliberately —
//!   upstream resolves it through `resolveQualifiedName`
//!   (`internal/checker/checker.go:15828`). This is where the 4,473 gap lines
//!   are.
//! - **The printing half.** A resolved `M.I` still has to be *named*. Two
//!   designs, and the difference between them is the whole finding:
//!   - **W — the written entity name.** Print the source's own dotted text, the
//!     mechanism `ff49871` already built for `typeof`
//!     ([`crate::signatures::Parameter::written_text`]) and which
//!     `Checker::unresolved_type_reference` (`declared.rs:732`) already
//!     implements for the *unresolvable* half of the same construct.
//!   - **R — the computed name.** Print the resolved type through
//!     `type_to_string`, which prints the symbol's own bare name.
//!   - **P — the symbol chain.** `getSymbolChain`
//!     (`internal/checker/nodebuilderimpl.go:1086`) with `needsQualification`
//!     (`internal/checker/symbolaccessibility.go:688`). This is the design that
//!     was built and reverted; only its **at-risk** column is re-measured here,
//!     because its conversion column (525 / 614 gained) is already on record.
//!
//! # The three columns
//!
//! - **CONVERTS** — the forecast string equals the baseline's right-hand side
//!   **character for character**. A match test, not a shape test.
//! - **WOULD PRINT WRONG** — the arm produces a type and it is not the
//!   baseline's. A gap turned into a wrong line.
//! - **AT RISK / WOULD BREAK** — lines that are **right today** and whose
//!   printed text this mechanism would change. `docs/conventions.md`: *"when a
//!   mechanism fires on a POSITION rather than on a defect, compute the at-risk
//!   population in the same pass that computes the target one"*. Qualified
//!   naming fires on every printed name, so this column is mandatory and it is
//!   the number that decides. It is computed for all three designs:
//!   - **W** — a right line whose printed text came from a written qualified
//!     name cannot move: W prints that same text.
//!   - **R** — the same lines **do** move: the written `M.I` becomes the
//!     computed `I`. Measured as the conjunction of two signals, because either
//!     alone over-counts: the declaration writes a qualified name whose root
//!     resolves as a namespace, **and** the printed answer contains that
//!     written text.
//!   - **P** — every right line whose answer *is* a namespace-declared name
//!     that does not resolve bare at the reference site. This is the 3,202.
//!
//! # Controls, with their expected values fixed before the run
//!
//! - **C1, construction.** Every classified target line's root reference
//!   answers `errorType` today. Expect **0** violations. It also carries the
//!   at-risk claim for the resolution half: an arm that only fires where the
//!   answer is `errorType` cannot move a line that is already right *through
//!   that reference's own printed form* — the W/R at-risk buckets below measure
//!   the route by which it can anyway.
//! - **C2, arithmetic.** Each design's buckets sum to the classified count.
//! - **C3, pinned by upstream.** Every classified reference's **leftmost**
//!   identifier resolves to a symbol carrying `SymbolFlags::NAMESPACE`. Its
//!   value comes from `resolveEntityName` (`checker.go:15772`), which resolves
//!   the left of a qualified name with meaning `SymbolFlagsNamespace` **only**
//!   (`resolveQualifiedName`, `checker.go:15829`) — so a left that is not a
//!   namespace makes upstream return nil and fall through to the
//!   unresolved-symbol path, which is a different bucket entirely. Expect
//!   **0** violations. A non-zero reading says the admission predicate in
//!   `declared.rs` and upstream's disagree, which would invalidate the
//!   population rather than move a count.
//! - **C4, pinned by upstream.** For a classified reference the probe's
//!   `resolve_entity_name` must find the right-hand name in the namespace
//!   symbol's `exports` table or decline — upstream reads exactly
//!   `c.getSymbol(c.getExportsOfSymbol(namespace), text, meaning)`
//!   (`checker.go:15851`). Every symbol it returns therefore has a
//!   declaration. Expect **0** resolved-but-undeclared.
//! - **C5, pinned by a named corpus case and a named upstream arm.**
//!   `checker-notes-nameres.md` §51: `conformance/parserRealSource10` lost
//!   **481** lines to design P because `resolve_name` could not see a
//!   namespace's exports, so `export enum TokenID` referenced from inside
//!   `namespace TypeScript` was invisible and the walk concluded a qualifier
//!   was needed. `resolve_name` gained that arm (`nameresolver.go:104`–`:146`)
//!   at `3b7fa44`. **Expect 0 at-risk-P lines in that case.** This is the
//!   control that decides whether the refusal's stated cause is actually gone;
//!   a non-zero reading means it is not, and the item stays refused for the
//!   reason it was refused before.
//! - **C6, positive control.** The at-risk-P *denominator* — right lines whose
//!   answer is a namespace-declared name at all — must be non-zero.
//!   `checker-notes-nameres.md` §48: a control reading zero proves nothing
//!   until a line can reach it. Reported unconditionally.
//! - **C7, positive control for the at-risk-R figure.** The design-R at-risk
//!   test asks whether a right line's printed answer contains a *written*
//!   qualified name. If **no** right line prints a dotted name whose root is a
//!   namespace at all, a zero there is vacuous rather than reassuring. Reported
//!   unconditionally, for the same §48 reason.
//!
//! # What this probe does not model, stated rather than implied
//!
//! - **A forecast is the reference's own printed text**, compared to the whole
//!   baseline right-hand side. Those coincide only when the reference *is* the
//!   line's answer, so two shapes are separated into their own buckets and
//!   counted in neither column — see [`judge`]: the reference reached through
//!   more than one `depend.rs` step, and the reference nested inside a larger
//!   printed type. Together they are a stated ceiling on further conversions
//!   and a floor of zero on further wrong.
//! - **Resolution has non-textual consequences.** A resolved `M.I` participates
//!   in assignability and property lookup, which can move lines this
//!   text-level counterfactual cannot see. That is the same limit every
//!   counterfactual on this board has, and it is the reason the observed
//!   conversion band is 15–57% rather than a point.
//! - **At-risk-P is reported as a strict and a loose bound together**, the
//!   shape `refmatch.rs` uses. Strict counts a right line whose *top-level*
//!   type is a namespace-declared name; loose additionally counts a right line
//!   any of whose printed identifier tokens names a namespace-declared symbol
//!   that does not resolve bare at the site. Strict under-counts unions and
//!   signatures; loose over-counts coincidental token matches.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{EntityName, Node, NodeId, NodeMap, NodeTable, SyntaxKind, push_children};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_checker::types::TypeData;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// `depend.rs`'s cap, copied with it.
const MAX_DEPTH: usize = 16;

/// The case `checker-notes-nameres.md` §51 names as the largest single loss of
/// the reverted design — **−481 lines**. C5's subject.
const C5_CASE: &str = "conformance/parserRealSource10";

// ---------------------------------------------------------------------------
// `depend.rs`'s walk, copied verbatim from `typerefgap.rs`.
//
// The target population is a `depend.rs` bucket, so it is defined by that
// walk. Copied rather than approximated: a re-implementation that drifts by one
// edge measures a different population and reports it under the published
// number, which is `bd tsr-qj4`'s failure mode arriving through another door.
// ---------------------------------------------------------------------------

/// Copied from `depend.rs`. Does this node's own answer gap?
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

/// Copied from `depend.rs`. One step toward what this node's answer depends on.
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
                EntityName::Identifier(identifier) => identifier.text,
                EntityName::QualifiedName(qualified) => qualified.right?.text,
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

// ---------------------------------------------------------------------------
// The probe's own model of the two upstream functions being sized.
// ---------------------------------------------------------------------------

/// The source spelling of an entity name — `A`, or `A.B.C`.
///
/// Replicates `Checker::entity_name_text` (`declared.rs:763`), which is private.
/// Replicated rather than exported: this is a measurement and it must not change
/// checker code. Upstream builds the same string with `getSymbolPath` over the
/// chain of unresolved parent symbols (`checker.go:23139`).
fn entity_name_text(name: Option<EntityName<'_>>) -> Option<String> {
    match name? {
        EntityName::Identifier(identifier) => Some(identifier.text.to_owned()),
        EntityName::QualifiedName(qualified) => {
            let left = entity_name_text(qualified.left)?;
            Some(format!("{left}.{}", qualified.right?.text))
        }
    }
}

/// The leftmost identifier of an entity name — what `resolveQualifiedName`
/// (`checker.go:15829`) resolves as a namespace before anything else.
fn leftmost(name: EntityName<'_>) -> Option<&tsr_ast::Identifier<'_>> {
    let mut root = name;
    while let EntityName::QualifiedName(inner) = root {
        root = inner.left?;
    }
    match root {
        EntityName::Identifier(identifier) => Some(identifier),
        EntityName::QualifiedName(_) => None,
    }
}

/// How many segments a dotted name has. `M.I` is 2.
fn entity_depth(name: EntityName<'_>) -> usize {
    match name {
        EntityName::Identifier(_) => 1,
        EntityName::QualifiedName(qualified) => {
            qualified.left.map_or(1, |left| 1 + entity_depth(left))
        }
    }
}

/// The probe's port of `resolveEntityName` (`checker.go:15772`), for the two
/// arms a type reference can take.
///
/// The resolution site is the name's own node, which is upstream's default:
/// `resolveEntityName` takes a `location` and falls back to `name` when it is
/// nil (`checker.go:15788`), and every call reaching a type reference passes
/// nil.
///
/// The qualified arm is `resolveQualifiedName` (`checker.go:15828`): resolve the
/// left with meaning `SymbolFlagsNamespace`, then look the right-hand text up in
/// `getExportsOfSymbol(namespace)` (`checker.go:15851`). Upstream's merge and
/// `export =` re-resolution are **not** modelled; the reasons they are not are
/// each reported as their own decline bucket, so an unmodelled arm shows up as a
/// refusal rather than as a silently missing conversion.
fn resolve_entity_name<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    name: EntityName<'a>,
    meaning: SymbolFlags,
) -> Result<SymbolId, &'static str> {
    match name {
        EntityName::Identifier(identifier) => {
            let id = identifier.node_id.ok_or("the name has no node id")?;
            binder
                .resolve_name(nodes, map, id, identifier.text, meaning)
                .ok_or("the leftmost name does not resolve")
        }
        EntityName::QualifiedName(qualified) => {
            let left = qualified.left.ok_or("malformed qualified name — no left")?;
            let namespace = resolve_entity_name(binder, nodes, map, left, SymbolFlags::NAMESPACE)?;
            let right = qualified.right.ok_or("malformed qualified name — no right")?;
            let found = *binder
                .symbols()
                .get(namespace)
                .exports
                .get(right.text)
                .ok_or("the namespace has no export of that name")?;
            let found = binder.merged_symbol(found);
            let flags = binder.symbols().get(found).flags;
            if !flags.intersects(meaning) && !flags.intersects(SymbolFlags::ALIAS) {
                return Err("the export exists but not with the wanted meaning");
            }
            Ok(found)
        }
    }
}

/// The type parameters a symbol's own declaration writes.
///
/// Replicates `Checker::local_type_parameters_of` (`declared.rs`), which is
/// private and whose count is the discriminator between the non-generic and the
/// instantiating branch of `get_type_from_type_reference`. Copied from
/// `typerefgap.rs`, which sized the population this probe forecasts against.
fn local_type_parameter_count<'a>(
    binder: &tsr_binder::BindResult<'a>,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> usize {
    let Some(declaration) = binder.symbols().get(symbol).declarations.first().copied() else {
        return 0;
    };
    match map.get(declaration) {
        Some(Node::ClassDeclaration(node)) => node.type_parameters.len(),
        Some(Node::ClassExpression(node)) => node.type_parameters.len(),
        Some(Node::InterfaceDeclaration(node)) => node.type_parameters.len(),
        Some(Node::TypeAliasDeclaration(node)) => node.type_parameters.len(),
        _ => 0,
    }
}

/// The nearest enclosing `ModuleDeclaration` of any of a symbol's declarations.
///
/// `Some` means the printed name is one `getSymbolChain`
/// (`nodebuilderimpl.go:1086`) would consider qualifying, because
/// `getContainersOfSymbol` (`symbolaccessibility.go:280`) walks declarations to
/// recover the container upstream leaves off `Symbol.Parent` for locals.
fn namespace_container<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> Option<NodeId> {
    for &declaration in &binder.symbols().get(symbol).declarations {
        let mut current = nodes.parent(declaration);
        while let Some(node) = current {
            if matches!(map.get(node), Some(Node::ModuleDeclaration(_))) {
                return Some(node);
            }
            current = nodes.parent(node);
        }
    }
    None
}

/// `needsQualification` (`symbolaccessibility.go:688`), reduced to the test the
/// reverted build used: the symbol's own name, resolved from the reference site,
/// is that same symbol.
///
/// Upstream iterates every symbol table in scope and answers *no qualification
/// needed* the moment one holds the symbol itself (`symbolaccessibility.go:697`,
/// `if symbolFromSymbolTable == symbol { return true }`). `resolve_name` walks
/// the same tables in the same order, so it answers the same question for the
/// first hit; it differs on the *later* tables upstream keeps scanning, which
/// can only make this over-report qualification.
fn needs_qualification<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
    name: &str,
    site: NodeId,
    meaning: SymbolFlags,
) -> bool {
    binder.resolve_name(nodes, map, site, name, meaning) != Some(symbol)
}

/// Whether `node` is inside the body of `container`.
fn is_inside(nodes: &NodeTable, node: NodeId, container: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == container {
            return true;
        }
        current = nodes.parent(id);
    }
    false
}

/// Every `TypeReferenceNode` in the *signature* part of a declaration —
/// annotations and parameter annotations, but not statement bodies.
///
/// The walk stops at a `Block` because a body's annotations are never part of
/// what the declaration's own line prints, and counting them would inflate the
/// at-risk column with lines the mechanism cannot reach.
fn signature_type_references<'a>(
    map: &NodeMap<'a>,
    root: NodeId,
    out: &mut Vec<&'a tsr_ast::TypeReferenceNode<'a>>,
) {
    let Some(node) = map.get(root) else { return };
    if matches!(node, Node::Block(_)) {
        return;
    }
    if let Node::TypeReferenceNode(reference) = node {
        out.push(reference);
    }
    let mut children = Vec::new();
    push_children(node, &mut children);
    for child in children {
        if let Some(id) = child.node_id() {
            signature_type_references(map, id, out);
        }
    }
}

/// Whether a printed type contains `A.B` where `A` resolves as a namespace at
/// `site`. C7's predicate: the population the design-R at-risk test draws from.
fn dotted_namespace_roots<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    site: NodeId,
    printed: &str,
) -> bool {
    let bytes = printed.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    let mut start = None;
    for (index, &byte) in bytes.iter().enumerate() {
        if ident(byte) {
            start.get_or_insert(index);
            continue;
        }
        if let Some(from) = start.take()
            && byte == b'.'
            && bytes.get(index + 1).is_some_and(|&next| ident(next))
            && !bytes[from].is_ascii_digit()
            && binder
                .resolve_name(nodes, map, site, &printed[from..index], SymbolFlags::NAMESPACE)
                .is_some()
        {
            return true;
        }
    }
    false
}

/// The identifier-shaped tokens of a printed type, for the loose at-risk bound.
fn name_tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|token| !token.is_empty() && !token.starts_with(|c: char| c.is_ascii_digit()))
        .collect()
}

// ---------------------------------------------------------------------------
// The report.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Report {
    /// Every aligned assertion line, by verdict today.
    right: usize,
    gap: usize,
    wrong: usize,

    // --- the target population -------------------------------------------
    classified: usize,
    /// How the `depend.rs` chain ended for each classified line.
    endings: BTreeMap<&'static str, usize>,
    /// Design W, one bucket per line.
    forms_w: BTreeMap<String, usize>,
    /// Design R, one bucket per line.
    forms_r: BTreeMap<String, usize>,
    /// `(sub-family, design, outcome)` — the split that prices a positional
    /// refusal, the `tsr-4sa` move.
    families: BTreeMap<(String, &'static str, &'static str), usize>,
    wrong_w: BTreeMap<String, usize>,
    wrong_r: BTreeMap<String, usize>,
    converted_cases_w: BTreeMap<String, usize>,
    converted_cases_r: BTreeMap<String, usize>,
    classified_cases: BTreeMap<String, usize>,
    wants_any: usize,

    // --- the at-risk population ------------------------------------------
    /// Right lines whose signature writes a qualified name that gaps today.
    at_risk_written: usize,
    at_risk_written_cases: BTreeMap<String, usize>,
    at_risk_written_sample: BTreeMap<String, usize>,
    /// C6's denominator: right lines whose answer *is* a namespace-declared
    /// name at all.
    p_denominator: usize,
    /// Design P, strict bound.
    at_risk_p_strict: usize,
    at_risk_p_strict_cases: BTreeMap<String, usize>,
    at_risk_p_sample: BTreeMap<String, usize>,
    /// Design P, loose bound.
    at_risk_p_loose: usize,
    at_risk_p_loose_cases: BTreeMap<String, usize>,
    at_risk_p_loose_names: BTreeMap<String, usize>,
    /// Right lines printing a bare name this probe could not attribute to a
    /// symbol — the stated blind spot of the strict bound.
    p_unattributable: usize,

    // --- controls ---------------------------------------------------------
    c1_root_does_not_gap: usize,
    c3_left_not_a_namespace: usize,
    c3_routes: BTreeMap<&'static str, usize>,
    /// How many `depend.rs` steps separate the assertion node from the root
    /// reference, capped at 4. `1` is the scorable population.
    depths: BTreeMap<usize, usize>,
    /// C7: right lines whose printed answer contains a dotted name whose root
    /// resolves as a namespace here — the population the design-R at-risk test
    /// draws from. Zero makes that test vacuous.
    c7_printed_dotted: usize,
    c4_resolved_but_undeclared: usize,
    c5_parser_real_source_10: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.right += other.right;
        self.gap += other.gap;
        self.wrong += other.wrong;
        self.classified += other.classified;
        self.wants_any += other.wants_any;
        self.at_risk_written += other.at_risk_written;
        self.p_denominator += other.p_denominator;
        self.at_risk_p_strict += other.at_risk_p_strict;
        self.at_risk_p_loose += other.at_risk_p_loose;
        self.p_unattributable += other.p_unattributable;
        self.c1_root_does_not_gap += other.c1_root_does_not_gap;
        self.c3_left_not_a_namespace += other.c3_left_not_a_namespace;
        self.c4_resolved_but_undeclared += other.c4_resolved_but_undeclared;
        self.c5_parser_real_source_10 += other.c5_parser_real_source_10;
        for (key, n) in &other.endings {
            *self.endings.entry(key).or_default() += n;
        }
        for (key, n) in &other.c3_routes {
            *self.c3_routes.entry(key).or_default() += n;
        }
        for (key, n) in &other.depths {
            *self.depths.entry(*key).or_default() += n;
        }
        self.c7_printed_dotted += other.c7_printed_dotted;
        for (key, n) in &other.families {
            *self.families.entry(key.clone()).or_default() += n;
        }
        for (target, source) in [
            (&mut self.forms_w, &other.forms_w),
            (&mut self.forms_r, &other.forms_r),
            (&mut self.wrong_w, &other.wrong_w),
            (&mut self.wrong_r, &other.wrong_r),
            (&mut self.converted_cases_w, &other.converted_cases_w),
            (&mut self.converted_cases_r, &other.converted_cases_r),
            (&mut self.classified_cases, &other.classified_cases),
            (&mut self.at_risk_written_cases, &other.at_risk_written_cases),
            (&mut self.at_risk_written_sample, &other.at_risk_written_sample),
            (&mut self.at_risk_p_strict_cases, &other.at_risk_p_strict_cases),
            (&mut self.at_risk_p_sample, &other.at_risk_p_sample),
            (&mut self.at_risk_p_loose_cases, &other.at_risk_p_loose_cases),
            (&mut self.at_risk_p_loose_names, &other.at_risk_p_loose_names),
        ] {
            for (key, n) in source {
                *target.entry(key.clone()).or_default() += n;
            }
        }
    }
}

/// What one design forecast for one line.
enum Outcome {
    Converts,
    Wrong { forecast: String },
    WantsAny,
    Nested,
    Indirect,
    Declines(String),
}

impl Outcome {
    fn label(&self) -> &'static str {
        match self {
            Outcome::Converts => "CONVERTS",
            Outcome::Wrong { .. } => "WOULD PRINT WRONG",
            Outcome::WantsAny => "want `any` (ADR-0038/0039 forbid it)",
            Outcome::Nested => "UNSCORED — the reference is nested inside the line's printed type",
            Outcome::Indirect => "UNSCORED — the reference is upstream of the line, not the line",
            Outcome::Declines(_) => "DECLINES",
        }
    }
}

/// The match test, with the two shapes the probe **cannot** score separated out
/// rather than counted as wrong.
///
/// The forecast is the *reference's* printed text and `wanted` is the whole
/// baseline right-hand side. Those coincide only when the reference is the
/// line's own answer:
///
/// - **`direct` is false** — `depend.rs`'s chain reached the reference through
///   more than one step, so the line's answer is something computed *from* the
///   reference (`_.isFunction` is a property of `Underscore.Static`). The
///   forecast says nothing about the line.
/// - **`wanted` contains the forecast** — the reference sits inside a larger
///   printed type (`() => privateModule.publicClass` around
///   `privateModule.publicClass`). Whether the wrapper prints right depends on
///   machinery this probe does not run.
///
/// Both are reported as their own buckets and counted in neither column. That
/// is deliberately the conservative reading of the **wrong** side and the
/// optimistic reading of the **converts** side, so the two buckets are also a
/// stated ceiling on additional conversions.
/// Whether `forecast` occurs inside `wanted` as a **whole printed type**, not
/// as a fragment of a longer name.
///
/// The boundary rule is the load-bearing part and it was wrong on the first
/// run. A plain `wanted.contains(forecast)` demoted
/// `want Underscore.Static / forecast Static` to *unscored*, when that is the
/// single most important **wrong** line design R produces: the baseline carries
/// the qualifier and R drops it. A `.` on either side of the occurrence means
/// exactly that — the baseline's name is longer than the forecast's *by a
/// qualifier* — so `.` is excluded as a boundary while every other
/// non-identifier character is accepted.
fn is_nested_occurrence(wanted: &str, forecast: &str) -> bool {
    if forecast.is_empty() {
        return false;
    }
    let boundary = |c: Option<char>| match c {
        None => true,
        Some(c) => !(c.is_alphanumeric() || c == '_' || c == '$' || c == '.'),
    };
    let mut from = 0;
    while let Some(offset) = wanted[from..].find(forecast) {
        let start = from + offset;
        let end = start + forecast.len();
        if boundary(wanted[..start].chars().next_back()) && boundary(wanted[end..].chars().next()) {
            return true;
        }
        from = start + 1;
    }
    false
}

fn judge(forecast: String, wanted: &str, direct: bool) -> Outcome {
    if forecast == wanted {
        return Outcome::Converts;
    }
    if !direct {
        return Outcome::Indirect;
    }
    if wanted == "any" {
        return Outcome::WantsAny;
    }
    if is_nested_occurrence(wanted, &forecast) {
        return Outcome::Nested;
    }
    Outcome::Wrong { forecast }
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
    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
    let error = checker.intrinsics().error;

    // The loose at-risk bound's index: every symbol declared inside a
    // `ModuleDeclaration`, by name. Built once per case.
    // Only symbols the node builder could ever *name* as a type: a symbol with
    // `SymbolFlags::TYPE`, minus type parameters, which
    // `lookupSymbolChainWorker` (`nodebuilderimpl.go:1069`) excludes from the
    // chain by construction (`isTypeParameter`). The first run indexed every
    // symbol declared inside a namespace and the loose bound's head became
    // `number`, `this`, `T`, `value` — locals and parameters whose names
    // collide with printed tokens, not names a chain would qualify.
    let mut namespaced: HashMap<&str, Vec<SymbolId>> = HashMap::new();
    for (id, symbol) in bound.symbols().iter() {
        if symbol.flags.intersects(SymbolFlags::TYPE)
            && !symbol.flags.intersects(SymbolFlags::TYPE_PARAMETER)
            && namespace_container(bound, nodes, map, id).is_some()
        {
            namespaced.entry(symbol.name).or_default().push(id);
        }
    }

    let mut report = Report::default();
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            let id = line_ids[position];

            // ---------------------------------------------------------------
            // AT RISK — lines that are RIGHT today. `docs/conventions.md`: the
            // at-risk population is computed in the SAME pass as the target
            // one, on lines this loop is already visiting.
            // ---------------------------------------------------------------
            if want.text == got.line() {
                report.right += 1;

                // (a) The resolution half. A right line whose signature writes
                // a qualified name that gaps today is a line whose text came
                // from somewhere other than that reference's computed type —
                // written-node reuse. Design W reprints the same written text
                // and cannot move it; design R replaces it with the computed
                // bare name and does.
                let declaration = nodes
                    .parent(id)
                    .filter(|&p| map.get(p).and_then(|n| n.name_id()) == Some(id))
                    .unwrap_or(id);
                let mut references = Vec::new();
                signature_type_references(map, declaration, &mut references);
                // **Two signals, conjoined.** A written qualified name whose
                // root resolves as a namespace is what design R would newly
                // resolve; the printed answer *containing that written text* is
                // what says the text on this line came from it. Either alone
                // over-counts: a class body writes annotations that never reach
                // the class's own printed name, and an enum member type prints
                // `E.A` without any qualified reference behind it.
                let at_risk_r = references.iter().any(|reference| {
                    let Some(name @ EntityName::QualifiedName(_)) = reference.type_name else {
                        return false;
                    };
                    let Some(root) = leftmost(name) else { return false };
                    let Some(root_id) = root.node_id else { return false };
                    if bound
                        .resolve_name(nodes, map, root_id, root.text, SymbolFlags::NAMESPACE)
                        .is_none()
                    {
                        return false;
                    }
                    entity_name_text(reference.type_name)
                        .is_some_and(|written| got.type_string.contains(&written))
                });
                // C7, the positive control for the test above: does any right
                // line print a dotted name whose root is a namespace here at
                // all? `checker-notes-nameres.md` §48 — a zero proves nothing
                // until a line can reach it.
                if dotted_namespace_roots(bound, nodes, map, id, &got.type_string) {
                    report.c7_printed_dotted += 1;
                }
                if at_risk_r {
                    report.at_risk_written += 1;
                    *report.at_risk_written_cases.entry(case.name.clone()).or_default() += 1;
                    *report
                        .at_risk_written_sample
                        .entry(format!("{}  [{}]", want.text, case.name))
                        .or_default() += 1;
                }

                // (b) The printing half, strict bound.
                //
                // **Gated exactly as the reverted build was gated.**
                // `checker-notes-nameres.md` §49: *"Gated so that it applies
                // only where the printed form IS the symbol's name — the same
                // textual test that defined the 1,318-line population — because
                // without that gate it climbs from an anonymous `__function`
                // symbol to no parent and gaps every function and object type
                // in the corpus."* Without this gate the bound counts every
                // signature and type literal whose symbol happens to live in a
                // namespace, which is not a name the chain would ever rewrite.
                let type_id =
                    types_producer::type_id_at_location(&mut checker, bound, nodes, map, id);
                let named = match &checker.type_of(type_id).data {
                    TypeData::Named { text, members } => Some((text.clone(), *members)),
                    TypeData::Anonymous { text, symbol, .. } => Some((text.clone(), Some(*symbol))),
                    _ => None,
                };
                if let Some((printed, symbol)) = named {
                    let bare = !printed.contains(['.', '<', ' ', '(', '{']);
                    match symbol {
                        Some(symbol)
                            if namespace_container(bound, nodes, map, symbol).is_some() =>
                        {
                            let name = bound.symbols().get(symbol).name;
                            let prints_as_the_name = printed == name
                                || printed.starts_with(&format!("{name}<"))
                                || printed == format!("typeof {name}");
                            if prints_as_the_name {
                                report.p_denominator += 1;
                                if needs_qualification(
                                    bound,
                                    nodes,
                                    map,
                                    symbol,
                                    name,
                                    id,
                                    SymbolFlags::TYPE | SymbolFlags::VALUE,
                                ) {
                                    report.at_risk_p_strict += 1;
                                    *report
                                        .at_risk_p_strict_cases
                                        .entry(case.name.clone())
                                        .or_default() += 1;
                                    *report
                                        .at_risk_p_sample
                                        .entry(format!("{}  [{}]", want.text, case.name))
                                        .or_default() += 1;
                                    if case.name == C5_CASE {
                                        report.c5_parser_real_source_10 += 1;
                                    }
                                }
                            }
                        }
                        // A bare name whose type carries no symbol — an enum or
                        // a type parameter (`new_named_type`, `declared.rs`).
                        // The strict bound cannot attribute it, and saying so
                        // is the honest form: this is the direction that makes
                        // the design look better than it is.
                        _ if bare => report.p_unattributable += 1,
                        _ => {}
                    }
                }

                // (c) The printing half, loose bound: any printed identifier
                // token that names a namespace-declared symbol which does not
                // resolve bare here.
                let printed = checker.type_to_string(type_id);
                let loose: Vec<&str> = name_tokens(&printed)
                    .into_iter()
                    .filter(|token| {
                        let Some(candidates) = namespaced.get(token) else { return false };
                        candidates.iter().any(|&symbol| {
                            needs_qualification(
                                bound,
                                nodes,
                                map,
                                symbol,
                                token,
                                id,
                                SymbolFlags::TYPE | SymbolFlags::VALUE,
                            )
                        })
                    })
                    .collect();
                if !loose.is_empty() {
                    report.at_risk_p_loose += 1;
                    *report.at_risk_p_loose_cases.entry(case.name.clone()).or_default() += 1;
                    for token in loose {
                        *report
                            .at_risk_p_loose_names
                            .entry(format!("{token}  [{}]", case.name))
                            .or_default() += 1;
                    }
                }
                continue;
            }

            // Unaligned: the baseline and this port disagree about the node
            // text, so no comparison of answers is meaningful.
            if want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                continue;
            }
            if got.type_string != "error" {
                report.wrong += 1;
                continue;
            }
            report.gap += 1;

            // ---------------------------------------------------------------
            // THE TARGET — `depend.rs`'s walk, verbatim.
            // ---------------------------------------------------------------
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
            if nodes.kind(current) != SyntaxKind::TypeReference {
                continue;
            }
            let Some(Node::TypeReferenceNode(reference)) = map.get(current) else { continue };
            let Some(name @ EntityName::QualifiedName(_)) = reference.type_name else { continue };
            let Some(root) = leftmost(name) else { continue };
            let Some(root_id) = root.node_id else { continue };
            let Some(root_symbol) =
                bound.resolve_name(nodes, map, root_id, root.text, SymbolFlags::NAMESPACE)
            else {
                // Not this family: upstream mints the unresolved symbol and
                // prints the dotted text, which this port already does
                // (`unresolved_type_reference`, `d356450`).
                continue;
            };

            report.classified += 1;
            *report.endings.entry(ending).or_default() += 1;
            *report.classified_cases.entry(case.name.clone()).or_default() += 1;
            if checker.get_type_from_type_node(tsr_ast::TypeNode::TypeReferenceNode(reference))
                != error
            {
                report.c1_root_does_not_gap += 1;
            }
            if !bound.symbols().get(root_symbol).flags.intersects(SymbolFlags::NAMESPACE) {
                report.c3_left_not_a_namespace += 1;
                // Which of `resolve_name`'s two unfiltered routes admitted it.
                // `lookup_scoped` accepts an `ALIAS` whatever its own flags say
                // (`crates/tsr-binder/src/lib.rs`), and the closing globals
                // lookup is not meaning-filtered at all.
                let flags = bound.symbols().get(root_symbol).flags;
                let route = if flags.intersects(SymbolFlags::ALIAS) {
                    "an ALIAS, accepted whatever its own flags say"
                } else if bound.globals().get(root.text) == Some(&root_symbol) {
                    "the globals table, which resolve_name does not meaning-filter"
                } else {
                    "neither an alias nor a global — unexplained"
                };
                *report.c3_routes.entry(route).or_default() += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();
            if wanted == "any" {
                report.wants_any += 1;
            }
            // One step from the assertion node means the reference **is** the
            // line's answer, so an exact match against the whole baseline
            // right-hand side is a valid test. More steps means the line's
            // answer was computed *from* the reference and the forecast says
            // nothing about it — see [`judge`].
            let direct = depth <= 1;
            *report.depths.entry(depth.min(4)).or_default() += 1;

            // The sub-families a positional refusal could be priced against.
            let inside =
                namespace_container(bound, nodes, map, root_symbol).is_some_and(|container| {
                    // The *namespace's own body*: the root symbol's container is
                    // the enclosing namespace of a nested one. The reference
                    // site being inside the namespace `M` itself is the case
                    // `needsQualification` answers "no qualifier" for.
                    is_inside(nodes, current, container)
                });
            let site_inside_namespace = bound
                .symbols()
                .get(root_symbol)
                .declarations
                .iter()
                .any(|&d| is_inside(nodes, current, d))
                || inside;
            let in_lib = bound
                .symbols()
                .get(root_symbol)
                .declarations
                .iter()
                .any(|&d| program.lib_files().iter().any(|file| file.contains(d)));
            let family = format!(
                "{} · {} · {} segments · {}",
                if in_lib { "lib namespace  " } else { "user namespace " },
                if site_inside_namespace {
                    "site INSIDE the namespace "
                } else {
                    "site outside            "
                },
                entity_depth(name),
                if reference.type_arguments.is_empty() {
                    "no type arguments"
                } else {
                    "type arguments   "
                },
            );

            // --- design W: the written entity name --------------------------
            //
            // Exactly `Checker::unresolved_type_reference` (`declared.rs:732`):
            // the dotted source text, plus resolved arguments, gapping whole if
            // any argument gaps.
            let outcome_w = 'w: {
                let Some(written) = entity_name_text(reference.type_name) else {
                    break 'w Outcome::Declines("the entity name has no text".to_string());
                };
                let mut printed = written;
                if !reference.type_arguments.is_empty() {
                    let mut arguments = Vec::new();
                    for argument in reference.type_arguments {
                        let id = checker.get_type_from_type_node(*argument);
                        if id == error {
                            break 'w Outcome::Declines(
                                "a written type ARGUMENT gaps — downstream".to_string(),
                            );
                        }
                        arguments.push(checker.type_to_string(id));
                    }
                    printed = format!("{printed}<{}>", arguments.join(", "));
                }
                // W still has to *resolve* to answer at all: without a symbol
                // there is no type, only a string.
                if let Err(reason) = resolve_entity_name(bound, nodes, map, name, SymbolFlags::TYPE)
                {
                    break 'w Outcome::Declines(format!("resolution fails — {reason}"));
                }
                judge(printed, &wanted, direct)
            };

            // --- design R: the computed name --------------------------------
            let outcome_r = 'r: {
                let symbol = match resolve_entity_name(bound, nodes, map, name, SymbolFlags::TYPE) {
                    Ok(symbol) => symbol,
                    Err(reason) => {
                        break 'r Outcome::Declines(format!("resolution fails — {reason}"));
                    }
                };
                if bound.symbols().get(symbol).declarations.is_empty() {
                    report.c4_resolved_but_undeclared += 1;
                    break 'r Outcome::Declines(
                        "C4 — resolved symbol has no declaration".to_string(),
                    );
                }
                let parameters = local_type_parameter_count(bound, map, symbol);
                if parameters == 0 && !reference.type_arguments.is_empty() {
                    break 'r Outcome::Declines(
                        "arguments on a non-generic type (checkNoTypeArguments)".to_string(),
                    );
                }
                if parameters > 0 {
                    // `get_instantiated_type_reference` is private and is a
                    // different item (`bd tsr-4qx`'s seam). Refused rather than
                    // approximated.
                    break 'r Outcome::Declines(
                        "generic target — instantiation is a separate item".to_string(),
                    );
                }
                let declared = checker.get_declared_type_of_symbol(symbol);
                if declared == error {
                    break 'r Outcome::Declines(
                        "DOWNSTREAM — the target's own declared type gaps".to_string(),
                    );
                }
                let regular = checker.get_regular_type_of_literal_type(declared);
                judge(checker.type_to_string(regular), &wanted, direct)
            };

            for (design, outcome, forms, wrongs, converted) in [
                (
                    "W",
                    &outcome_w,
                    &mut report.forms_w,
                    &mut report.wrong_w,
                    &mut report.converted_cases_w,
                ),
                (
                    "R",
                    &outcome_r,
                    &mut report.forms_r,
                    &mut report.wrong_r,
                    &mut report.converted_cases_r,
                ),
            ] {
                let bucket = match outcome {
                    Outcome::Declines(reason) => format!("DECLINES — {reason}"),
                    other => other.label().to_string(),
                };
                *forms.entry(bucket).or_default() += 1;
                *report.families.entry((family.clone(), design, outcome.label())).or_default() += 1;
                match outcome {
                    Outcome::Converts => {
                        *converted.entry(case.name.clone()).or_default() += 1;
                    }
                    Outcome::Wrong { forecast } => {
                        *wrongs
                            .entry(format!(
                                "want `{wanted}`, forecast `{forecast}`  [{}]",
                                case.name
                            ))
                            .or_default() += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn percent(part: usize, whole: usize) -> f64 {
    100.0 * part as f64 / whole as f64
}

fn print_map(title: &str, map: &BTreeMap<String, usize>, take: usize) {
    println!("\n## {title}\n");
    let mut rows: Vec<_> = map.iter().collect();
    rows.sort_by_key(|(key, n)| (std::cmp::Reverse(**n), (*key).clone()));
    let total: usize = map.values().sum();
    for (key, n) in rows.iter().take(take) {
        println!("  {n:>6}  {key}");
    }
    match rows.first() {
        Some((key, n)) => {
            let share = if total == 0 { 0.0 } else { percent(**n, total) };
            println!("  -- total {total}, top-1 {n} = {share:.1}%  ({key})");
        }
        None => println!("  -- total 0"),
    }
}

fn column(forms: &BTreeMap<String, usize>, label: &str) -> usize {
    forms.iter().filter(|(key, _)| key.starts_with(label)).map(|(_, n)| n).sum()
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");
    let mut report = Report::default();
    for partial in cases.par_iter().filter_map(measure).collect::<Vec<_>>() {
        report.merge(&partial);
    }

    println!("# qualname — the fresh counterfactual for namespace-qualified naming\n");
    println!(
        "aligned lines today:  right {}  gap {}  wrong {}",
        report.right, report.gap, report.wrong
    );
    println!(
        "classified target (gap line, root is a qualified TypeReference whose leftmost resolves as a namespace): {}",
        report.classified
    );
    println!("  of which want `any`: {}", report.wants_any);
    println!("\n  the depend.rs ending each classified line arrived by:");
    for (ending, n) in &report.endings {
        println!("    {n:>6}  {ending}");
    }

    println!("\n## The three columns\n");
    println!(
        "  converting cases: W {}, R {}; classified cases {}",
        report.converted_cases_w.len(),
        report.converted_cases_r.len(),
        report.classified_cases.len()
    );
    println!("  design                       CONVERTS   WOULD PRINT WRONG   want-any   DECLINES");
    for (design, forms) in
        [("W  written entity name", &report.forms_w), ("R  computed bare name ", &report.forms_r)]
    {
        println!(
            "  {design}   {:>8}   {:>17}   {:>8}   {:>8}",
            column(forms, "CONVERTS"),
            column(forms, "WOULD PRINT WRONG"),
            column(forms, "want `any`"),
            column(forms, "DECLINES"),
        );
    }

    println!("\n## AT RISK — lines that are RIGHT today and whose text this mechanism moves\n");
    println!(
        "  resolution half, design W (reprints the written text)              0  by construction"
    );
    println!(
        "  resolution half, design R (replaces written text with bare name)  {:>6}",
        report.at_risk_written
    );
    println!(
        "  printing half,   design P — STRICT bound                          {:>6}",
        report.at_risk_p_strict
    );
    println!(
        "  printing half,   design P — LOOSE bound                           {:>6}",
        report.at_risk_p_loose
    );
    println!(
        "  P denominator (right lines naming a namespace-declared symbol)    {:>6}  (C6)",
        report.p_denominator
    );
    println!(
        "  P unattributable (bare name, type carries no symbol)              {:>6}",
        report.p_unattributable
    );
    println!(
        "  cases touched: R-written {}, P-strict {}, P-loose {}",
        report.at_risk_written_cases.len(),
        report.at_risk_p_strict_cases.len(),
        report.at_risk_p_loose_cases.len(),
    );

    println!("\n## Controls\n");
    println!(
        "  C1  classified root does not gap                  {:>6}   (expect 0)",
        report.c1_root_does_not_gap
    );
    println!(
        "  C2  W buckets sum {} / R buckets sum {} vs classified {}",
        report.forms_w.values().sum::<usize>(),
        report.forms_r.values().sum::<usize>(),
        report.classified
    );
    println!(
        "  C3  leftmost resolves WITHOUT SymbolFlags::NAMESPACE  {:>4}   (expect 0, pinned by checker.go:15829)",
        report.c3_left_not_a_namespace
    );
    for (route, n) in &report.c3_routes {
        println!("        {n:>6}  via {route}");
    }
    println!(
        "  C4  resolved export symbol with no declaration    {:>6}   (expect 0, pinned by checker.go:15851)",
        report.c4_resolved_but_undeclared
    );
    println!(
        "  C5  at-risk-P lines in {C5_CASE}  {:>4}   (expect 0; was -481, nameres.md §51)",
        report.c5_parser_real_source_10
    );
    println!(
        "  C6  P denominator                                 {:>6}   (must be > 0 for C5 to be readable)",
        report.p_denominator
    );
    println!(
        "  C7  right lines printing a dotted namespace name  {:>6}   (must be > 0 for the design-R at-risk figure to mean anything)",
        report.c7_printed_dotted
    );

    println!("\n## Chain distance from the assertion line to the root reference\n");
    for (depth, n) in &report.depths {
        let label = if *depth == 1 { "  <- the scorable population" } else { "" };
        println!("  {n:>6}  {depth} step(s){label}");
    }

    print_map("Design W — every bucket", &report.forms_w, 30);
    print_map("Design R — every bucket", &report.forms_r, 30);
    print_map("Design W — the would-be-wrong lines, verbatim", &report.wrong_w, 25);
    print_map("Design R — the would-be-wrong lines, verbatim", &report.wrong_r, 25);
    print_map(
        "Design W — converting cases (the regressed-cases input)",
        &report.converted_cases_w,
        15,
    );
    print_map("Design R — converting cases", &report.converted_cases_r, 15);
    print_map("Classified cases", &report.classified_cases, 15);
    print_map("AT RISK (design R, written-text lines) — cases", &report.at_risk_written_cases, 15);
    print_map(
        "AT RISK (design R, written-text lines) — verbatim",
        &report.at_risk_written_sample,
        20,
    );
    print_map("AT RISK (design P, strict) — cases", &report.at_risk_p_strict_cases, 15);
    print_map("AT RISK (design P, strict) — verbatim", &report.at_risk_p_sample, 20);
    print_map("AT RISK (design P, loose) — cases", &report.at_risk_p_loose_cases, 15);
    print_map(
        "AT RISK (design P, loose) — the names driving it",
        &report.at_risk_p_loose_names,
        20,
    );

    println!("\n## The sub-family split — what a positional refusal would cost and save\n");
    let mut families: BTreeMap<String, BTreeMap<(&str, &str), usize>> = BTreeMap::new();
    for ((family, design, outcome), n) in &report.families {
        *families.entry(family.clone()).or_default().entry((design, outcome)).or_default() += n;
    }
    let mut ordered: Vec<_> = families.iter().collect();
    ordered.sort_by_key(|(_, cells)| std::cmp::Reverse(cells.values().sum::<usize>()));
    println!(
        "  {:<70}  {:>7} {:>7} {:>7} {:>7}",
        "sub-family", "W conv", "W wrong", "R conv", "R wrong"
    );
    for (family, cells) in ordered {
        let get = |design, outcome| cells.get(&(design, outcome)).copied().unwrap_or(0);
        println!(
            "  {family:<70}  {:>7} {:>7} {:>7} {:>7}",
            get("W", "CONVERTS"),
            get("W", "WOULD PRINT WRONG"),
            get("R", "CONVERTS"),
            get("R", "WOULD PRINT WRONG"),
        );
    }

    // The union of every case a design touches, which is what a `casedelta`
    // regression estimate is taken over.
    let touched: BTreeSet<&String> = report
        .converted_cases_w
        .keys()
        .chain(report.at_risk_written_cases.keys())
        .chain(report.at_risk_p_strict_cases.keys())
        .collect();
    println!("\n  cases any design touches: {}", touched.len());
}
