//! **Design P, re-sized against the compiler design W left behind.**
//!
//! ```text
//! cargo run --release -p tsr-conformance --example qualnamep
//! ```
//!
//! # Why this is a new instrument and not a column on `qualname.rs`
//!
//! `qualname.rs` prices the **resolution** half of namespace-qualified naming:
//! designs W and R both fire on a **gap** line — a qualified type reference this
//! port answers `errorType` for — and turn it into a printed type. Their target
//! population is therefore a `depend.rs` gap bucket, and their CONVERTS column
//! counts gap→right.
//!
//! **Design P cannot convert a gap line at all.** P is upstream's
//! `getSymbolChain` (`internal/checker/nodebuilderimpl.go:1087`) plus
//! `needsQualification` (`internal/checker/symbolaccessibility.go:688`): it
//! changes the *text* a type prints under when that type's symbol is declared
//! inside a namespace and the symbol's own name does not resolve to it at the
//! reference site. A line whose answer is `error` has no symbol to qualify. So
//! P's conversions are **wrong→right**, its losses are **right→wrong**, and its
//! third bucket is wrong→a *different* wrong, which converts nothing and costs
//! nothing.
//!
//! That is a different arithmetic from W's and it needs its own columns:
//!
//! | column | what it counts |
//! |---|---|
//! | **CONVERTS** | wrong today; P's forecast text **is** the baseline's |
//! | **CHURN** | wrong today; P changes the text and it is still not the baseline's |
//! | **AT RISK** | **right today**; P changes the text, so the line is lost |
//! | NO-OP | P's forecast equals what is printed now |
//! | GAP | `error` today — P cannot reach it. Expected to contribute nothing |
//!
//! For W the at-risk column was a side condition that read zero. **For P the
//! at-risk column is one of the two terms of the trade**, because P's gains and
//! P's losses come out of the same predicate. Cycle 20b measured that trade at
//! 525 gained against 3,202 lost.
//!
//! # Every P number on record is pre-W and none of them is carried
//!
//! - the conversion column 525/614 is from **cycle 20b**, many builds ago;
//! - the at-risk column — 36 strict, 4,801 loose — was measured by `qualname.rs`
//!   at `22a5a51`, **before design W landed at `8e28971` (+3,590)**;
//! - `checker-notes-qualname.md` §9.3 records that W's conversions **created** P
//!   population: 13 lines of that build's residual are `Foo.B<Foo.A>` wanted
//!   against `B<Foo.A>` printed — the missing chain on the *outer* name, newly
//!   visible because the inner argument resolved.
//!
//! So all three columns are re-taken here in one pass, on a compiler that has W.
//!
//! # The model of `getSymbolChain`, and where it deliberately stops short
//!
//! [`symbol_chain`] is the reduction of `getSymbolChain`
//! (`nodebuilderimpl.go:1087`) that a build would actually write:
//!
//! 1. If `needsQualification` says no (`nodebuilderimpl.go:1094`), the chain is
//!    the symbol alone. Modelled by [`needs_qualification`], which is
//!    `resolve_name` at the site — upstream's `someSymbolTableInScope` walk
//!    answering *no qualifier* the moment a table in scope holds the symbol
//!    itself (`symbolaccessibility.go:702`).
//! 2. Otherwise go up to the container and recurse with
//!    `getQualifiedLeftMeaning` — `SymbolFlagsNamespace`
//!    (`nodebuilderimpl.go:1111`).
//!
//! The container is `getContainersOfSymbol` (`symbolaccessibility.go:280`),
//! whose **first and normal** answer is `getParentOfSymbol`
//! (`checker.go:14365`) — i.e. `Symbol.Parent`, which the binder sets only for a
//! symbol that lives in another symbol's table. This probe uses
//! `Symbol::parent` for exactly that reason, and reports the **AST-walk**
//! definition `qualname.rs` used (`namespace_container`, a walk to the nearest
//! enclosing `ModuleDeclaration`) as a separate, looser figure — CP6. The two
//! differ precisely on a **non-exported** local of a namespace, which upstream
//! gives no container and therefore never qualifies.
//!
//! Two arms are **declined rather than approximated**, each as its own bucket:
//!
//! - **a container that is an external module** (a source file symbol).
//!   Upstream does not print a dotted name for it; it calls
//!   `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:1104`) and prints an
//!   import specifier. That is `bd tsr-4jk`'s family and it is the shape
//!   `checker-notes-novaldecl.md` refused on naming grounds.
//! - **the alternative containers** — `getWithAlternativeContainers`
//!   (`symbolaccessibility.go:117`), re-export and `export =` routes. Modelling
//!   them can only *add* chains, so declining them makes both the CONVERTS and
//!   the AT-RISK column a floor rather than a forecast, in the same direction.
//!
//! # Controls, values fixed before the run
//!
//! - **CP1, pinned by upstream. Expect 0.** No forecast may qualify a **type
//!   parameter**: `lookupSymbolChainWorker` returns the bare symbol for one by
//!   construction (`nodebuilderimpl.go:1069`, `isTypeParameter`). A non-zero
//!   reading says the gate is not the gate upstream has.
//! - **CP2, pinned by upstream. Expect 0.** A symbol for which
//!   `needsQualification` is false must produce a forecast identical to what is
//!   printed now — that is the whole content of the test at
//!   `nodebuilderimpl.go:1094`. A non-zero reading says the chain is being built
//!   past its own stop condition, which is the defect
//!   `checker-notes-nameres.md` §51 attributed the 3,202 lost lines to.
//! - **CP3, arithmetic.** The five buckets sum to the aligned line count.
//! - **CP4, pinned by the PREVIOUS BUILD's recorded residual. Expect ≥ 13.**
//!   `checker-notes-qualname.md` §9.3 names 13 residual lines as *"the missing
//!   symbol chain on the OUTER name, newly visible because the inner argument
//!   resolved"*, in `declFileGenericType`, `genericClassesInModule`,
//!   `variableDeclaratorResolved…`, `TwoInternalModules…`,
//!   `ramdaToolsNoInfinite` and `typeNamedUndefined1,2`. Those lines are wrong
//!   today and P is the named owner. **If P converts none of them this
//!   instrument is not modelling the mechanism §9.3 saw**, and every other
//!   number it prints is unreadable. This is the control that makes the run
//!   post-W rather than nominally post-W.
//!
//!   *Recorded because it was got wrong once:* the first run read **12** and the
//!   defect was in this list, not in the mechanism — §9.3 truncates one case to
//!   `variableDeclaratorResolved…` and it was expanded here to
//!   `…ModuleName`, which is not a corpus case. The corpus case is
//!   `variableDeclaratorResolvedDuringContextualTyping` and it converts **9**.
//!   The registered value is not restated; only the subject list is corrected.
//! - **CP5, the descendant of `qualname.rs`'s C5. Expect 0.**
//!   `conformance/parserRealSource10` lost **481** lines to the reverted design
//!   because `resolve_name` could not see a namespace's exports
//!   (`checker-notes-nameres.md` §51); the arm landed at `3b7fa44`. Expect 0
//!   at-risk lines in that case. Non-zero means the stated cause is back.
//! - **CP6, positive control and cross-check. Must be > 0.** The at-risk
//!   population under `qualname.rs`'s looser AST-walk container definition. It
//!   read **36** pre-W and can only have grown, since W added right lines and
//!   removed none. A reading below 36 says the two runs are not measuring the
//!   same corpus.
//! - **CP7, positive control that this tree HAS design W. Must be > 0.** Lines
//!   that are **right today** and whose printed answer is a dotted name — W's
//!   own output is the bulk of it. A zero here means the working tree predates
//!   `8e28971` and every column below is the pre-W measurement again.
//!
//! # What this probe does not model
//!
//! - **The loose bound rewrites tokens, not types.** Strict qualifies only a
//!   line whose whole printed answer *is* a symbol's name — the gate the
//!   reverted build used and which `checker-notes-nameres.md` §49 records as
//!   load-bearing (*"without that gate it climbs from an anonymous `__function`
//!   symbol to no parent and gaps every function and object type in the
//!   corpus"*). Loose additionally rewrites identifier tokens inside signature
//!   and type-literal renderings, which is an upper bound on a **fully**
//!   qualifying node builder rather than on the design priced here.
//! - **Non-textual consequences.** A qualified name is still the same type, so
//!   unlike W this mechanism has no downstream unlock at all — which is the one
//!   respect in which P is easier to price than W was.

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_checker::types::TypeData;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The case `checker-notes-nameres.md` §51 names as the reverted design's
/// largest single loss — **−481 lines**. CP5's subject.
const CP5_CASE: &str = "conformance/parserRealSource10";

/// The cases `checker-notes-qualname.md` §9.3 attributes to design P in the W
/// build's residual — *"the missing symbol chain on the OUTER name, newly
/// visible because the inner argument resolved"*, 13 lines. CP4's subjects.
const CP4_CASES: &[&str] = &[
    "declFileGenericType",
    "genericClassesInModule",
    // §9.3 truncates this one to `variableDeclaratorResolved…`; the corpus case
    // is `variableDeclaratorResolvedDuringContextualTyping`.
    "variableDeclaratorResolvedDuringContextualTyping",
    "TwoInternalModulesThatMergeEachWithExportedAndNonExportedInterfacesOfTheSameName",
    "ramdaToolsNoInfinite",
    "typeNamedUndefined1",
    "typeNamedUndefined2",
];

/// Chain depth cap. Upstream has none; a cycle in `Symbol.Parent` would be a
/// binder defect, and the guard exists so that one does not hang the corpus.
const MAX_CHAIN: usize = 8;

// ---------------------------------------------------------------------------
// The probe's model of the two upstream functions.
// ---------------------------------------------------------------------------

/// `needsQualification` (`symbolaccessibility.go:688`), reduced to the test the
/// reverted build used and `qualname.rs` measured with: does the symbol's own
/// name, resolved from the reference site, come back as that same symbol?
///
/// Upstream walks every symbol table in scope and answers *no qualification
/// needed* the moment one holds the symbol itself (`symbolaccessibility.go:702`,
/// `if symbolFromSymbolTable == symbol { return true }`). `resolve_name` walks
/// the same tables in the same order, so it agrees on the first hit and differs
/// only on the later tables upstream keeps scanning — which can only make this
/// **over**-report qualification, i.e. over-report both columns together.
fn needs_qualification<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
    name: &str,
    site: NodeId,
    meaning: SymbolFlags,
) -> bool {
    match binder.resolve_name(nodes, map, site, name, meaning) {
        Some(found) => binder.merged_symbol(found) != binder.merged_symbol(symbol),
        None => true,
    }
}

/// Is this symbol an external module — a source file, or an ambient
/// `declare module "x"`?
///
/// Upstream prints a chain through one of these with
/// `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:1104`), an import
/// specifier and not a dotted name. Declined here, as its own bucket.
fn is_external_module<'a>(
    binder: &tsr_binder::BindResult<'a>,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> bool {
    binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
        match map.get(declaration) {
            Some(Node::SourceFile(_)) => true,
            Some(Node::ModuleDeclaration(module)) => module
                .name
                .is_some_and(|name| matches!(name, tsr_ast::ModuleName::StringLiteral(_))),
            _ => false,
        }
    })
}

/// Why a chain could not be built.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Decline {
    /// `needsQualification` said no — the bare name is right. Not a shortfall.
    NoQualifierNeeded,
    /// `getContainersOfSymbol` has nothing: `Symbol.Parent` is unset, which is
    /// what upstream records for a **local** of a namespace.
    NoContainer,
    /// The container is an external module — `getSpecifierForModuleSymbol`.
    ExternalModule,
    /// The container is not a namespace: a class's static side, an enum, an
    /// interface's members table. `getQualifiedLeftMeaning` asks for
    /// `SymbolFlagsNamespace` (`nodebuilderimpl.go:1111`).
    ContainerNotANamespace,
    /// The chain cap tripped.
    TooDeep,
}

impl Decline {
    fn label(self) -> &'static str {
        match self {
            Decline::NoQualifierNeeded => "no qualifier needed (needsQualification = false)",
            Decline::NoContainer => "no container — Symbol.Parent unset (a namespace LOCAL)",
            Decline::ExternalModule => {
                "container is an external module — getSpecifierForModuleSymbol"
            }
            Decline::ContainerNotANamespace => "container is not a namespace",
            Decline::TooDeep => "chain cap",
        }
    }
}

/// `getSymbolChain` (`nodebuilderimpl.go:1087`), reduced to the arms a build
/// would write. Returns the dotted qualifier *prefix* — `Ok("M.")`, `Ok("A.B.")`
/// — or the reason there is none.
fn symbol_chain<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
    site: NodeId,
    meaning: SymbolFlags,
    depth: usize,
) -> Result<String, Decline> {
    if depth >= MAX_CHAIN {
        return Err(Decline::TooDeep);
    }
    let name = binder.symbols().get(symbol).name;
    if !needs_qualification(binder, nodes, map, symbol, name, site, meaning) {
        return Err(Decline::NoQualifierNeeded);
    }
    // `getContainersOfSymbol` (`symbolaccessibility.go:280`) — its first and
    // normal answer, `getParentOfSymbol` (`checker.go:14365`).
    let Some(parent) = binder.symbols().get(symbol).parent.map(|p| binder.merged_symbol(p)) else {
        return Err(Decline::NoContainer);
    };
    if is_external_module(binder, map, parent) {
        return Err(Decline::ExternalModule);
    }
    if !binder.symbols().get(parent).flags.intersects(SymbolFlags::MODULE) {
        return Err(Decline::ContainerNotANamespace);
    }
    let parent_name = binder.symbols().get(parent).name;
    // `getQualifiedLeftMeaning(meaning)` is `SymbolFlagsNamespace`
    // (`nodebuilderimpl.go:1111`).
    match symbol_chain(binder, nodes, map, parent, site, SymbolFlags::NAMESPACE, depth + 1) {
        Ok(prefix) => Ok(format!("{prefix}{parent_name}.")),
        // The parent itself does not need qualifying: the chain stops there,
        // which is the recursion's base case and not a decline.
        Err(Decline::NoQualifierNeeded) => Ok(format!("{parent_name}.")),
        Err(other) => Err(other),
    }
}

/// The nearest enclosing `ModuleDeclaration` of any of a symbol's declarations
/// — the container definition `qualname.rs` used. CP6's, and looser than
/// upstream's by exactly the non-exported locals.
fn ast_namespace_container<'a>(
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

/// Whether `node` is inside `container`.
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

/// The identifier-shaped tokens of a printed type, for the loose bound.
fn name_tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|token| !token.is_empty() && !token.starts_with(|c: char| c.is_ascii_digit()))
        .collect()
}

/// Does `printed` contain `A.B` where `A` resolves as a namespace at `site`?
/// CP7's predicate — design W's own output, which is what makes this run
/// post-W rather than nominally post-W.
fn dotted_namespace_root<'a>(
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

// ---------------------------------------------------------------------------
// The report.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Report {
    aligned: usize,
    right: usize,
    wrong: usize,
    gap: usize,

    // --- strict: P rewrites a line whose whole printed answer IS a name -----
    /// Right lines where the strict gate holds and a container exists at all —
    /// the denominator the at-risk figure is a share of.
    p_denominator: usize,
    converts: usize,
    converts_lines: BTreeMap<String, usize>,
    converts_cases: BTreeMap<String, usize>,
    churn: usize,
    churn_lines: BTreeMap<String, usize>,
    churn_cases: BTreeMap<String, usize>,
    at_risk: usize,
    at_risk_lines: BTreeMap<String, usize>,
    at_risk_cases: BTreeMap<String, usize>,
    /// `(sub-family, outcome)` — what a positional refusal costs and saves.
    families: BTreeMap<(String, &'static str), usize>,
    declines: BTreeMap<&'static str, usize>,

    // --- loose: token rewriting inside composite prints ---------------------
    loose_converts: usize,
    loose_converts_lines: BTreeMap<String, usize>,
    loose_churn: usize,
    loose_at_risk: usize,
    loose_at_risk_cases: BTreeMap<String, usize>,
    loose_at_risk_lines: BTreeMap<String, usize>,

    // --- controls -----------------------------------------------------------
    cp1_type_parameter_qualified: usize,
    cp2_chain_past_stop: usize,
    cp4_hits: BTreeMap<String, usize>,
    cp5_parser_real_source_10: usize,
    cp6_ast_walk_at_risk: usize,
    /// CP6 again, with `needs_qualification` comparing symbol ids **without**
    /// `getMergedSymbol` — which is what `qualname.rs` does. The difference
    /// between the two is the merged-declaration population.
    cp6_unmerged: usize,
    cp6_unmerged_lines: BTreeMap<String, usize>,
    /// Every line in one of CP4's named cases whose printed answer is a name P
    /// looked at, and what became of it.
    cp4_audit: BTreeMap<String, usize>,
    cp7_right_dotted: usize,
    /// Gap lines the strict path reached. Zero by construction — `error` names
    /// no symbol — and reported so the construction claim is measured.
    gap_reached: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        for (target, source) in [
            (&mut self.aligned, other.aligned),
            (&mut self.right, other.right),
            (&mut self.wrong, other.wrong),
            (&mut self.gap, other.gap),
            (&mut self.p_denominator, other.p_denominator),
            (&mut self.converts, other.converts),
            (&mut self.churn, other.churn),
            (&mut self.at_risk, other.at_risk),
            (&mut self.loose_converts, other.loose_converts),
            (&mut self.loose_churn, other.loose_churn),
            (&mut self.loose_at_risk, other.loose_at_risk),
            (&mut self.cp1_type_parameter_qualified, other.cp1_type_parameter_qualified),
            (&mut self.cp2_chain_past_stop, other.cp2_chain_past_stop),
            (&mut self.cp5_parser_real_source_10, other.cp5_parser_real_source_10),
            (&mut self.cp6_ast_walk_at_risk, other.cp6_ast_walk_at_risk),
            (&mut self.cp6_unmerged, other.cp6_unmerged),
            (&mut self.cp7_right_dotted, other.cp7_right_dotted),
            (&mut self.gap_reached, other.gap_reached),
        ] {
            *target += source;
        }
        for (key, n) in &other.declines {
            *self.declines.entry(key).or_default() += n;
        }
        for (key, n) in &other.families {
            *self.families.entry(key.clone()).or_default() += n;
        }
        for (target, source) in [
            (&mut self.converts_lines, &other.converts_lines),
            (&mut self.converts_cases, &other.converts_cases),
            (&mut self.churn_lines, &other.churn_lines),
            (&mut self.churn_cases, &other.churn_cases),
            (&mut self.at_risk_lines, &other.at_risk_lines),
            (&mut self.at_risk_cases, &other.at_risk_cases),
            (&mut self.loose_converts_lines, &other.loose_converts_lines),
            (&mut self.loose_at_risk_cases, &other.loose_at_risk_cases),
            (&mut self.loose_at_risk_lines, &other.loose_at_risk_lines),
            (&mut self.cp4_hits, &other.cp4_hits),
            (&mut self.cp4_audit, &other.cp4_audit),
            (&mut self.cp6_unmerged_lines, &other.cp6_unmerged_lines),
        ] {
            for (key, n) in source {
                *target.entry(key.clone()).or_default() += n;
            }
        }
    }
}

/// The strict gate: is `printed` this symbol's own name, possibly with type
/// arguments or under `typeof`? Returns the prefix and suffix around the name.
///
/// `checker-notes-nameres.md` §49 records this gate as the design: *"Gated so
/// that it applies only where the printed form IS the symbol's name … because
/// without that gate it climbs from an anonymous `__function` symbol to no
/// parent and gaps every function and object type in the corpus."*
fn split_around_name<'t>(printed: &'t str, name: &str) -> Option<(&'static str, &'t str)> {
    if name.is_empty() {
        return None;
    }
    if let Some(rest) = printed.strip_prefix("typeof ") {
        return if rest == name { Some(("typeof ", "")) } else { None };
    }
    if printed == name {
        return Some(("", ""));
    }
    printed.strip_prefix(name).filter(|rest| rest.starts_with('<')).map(|rest| ("", rest))
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

    // The loose bound's index: every symbol the node builder could *name* as a
    // type and that has a namespace container. `lookupSymbolChainWorker`
    // excludes type parameters by construction (`nodebuilderimpl.go:1069`), and
    // `qualname.rs` §5 records what happens without that restriction — the
    // bound's head became `number`, `this`, `T`, `value` and it read 111,321.
    let mut namespaced: HashMap<&str, Vec<SymbolId>> = HashMap::new();
    for (id, symbol) in bound.symbols().iter() {
        if symbol.flags.intersects(SymbolFlags::TYPE)
            && !symbol.flags.intersects(SymbolFlags::TYPE_PARAMETER)
            && symbol
                .parent
                .map(|p| bound.merged_symbol(p))
                .is_some_and(|p| bound.symbols().get(p).flags.intersects(SymbolFlags::MODULE))
        {
            namespaced.entry(symbol.name).or_default().push(id);
        }
    }

    let mut report = Report::default();
    let short = case.name.rsplit_once('/').map_or(case.name.as_str(), |(_, n)| n);
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            let id = line_ids[position];

            let is_right = want.text == got.line();
            if !is_right && want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                // Unaligned: the two sides disagree about the node text, so no
                // comparison of answers is meaningful.
                continue;
            }
            report.aligned += 1;
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();
            let printed = got.type_string.clone();
            let is_gap = !is_right && printed == "error";
            if is_right {
                report.right += 1;
                if dotted_namespace_root(bound, nodes, map, id, &printed) {
                    report.cp7_right_dotted += 1;
                }
            } else if is_gap {
                report.gap += 1;
            } else {
                report.wrong += 1;
            }

            // ---------------------------------------------------------------
            // STRICT — P rewrites a line whose whole printed answer IS a name.
            // ---------------------------------------------------------------
            let type_id = types_producer::type_id_at_location(&mut checker, bound, nodes, map, id);
            let named = match &checker.type_of(type_id).data {
                TypeData::Named { text, members } => Some((text.clone(), *members)),
                TypeData::Anonymous { text, symbol, .. } => Some((text.clone(), Some(*symbol))),
                _ => None,
            };
            // CP6, replicating `qualname.rs`'s at-risk-P block **verbatim** so
            // the two runs are comparable: its gate reads the `TypeData` text,
            // not the printed assertion line, and its container is the AST walk.
            if is_right && let Some((data_text, Some(symbol))) = named.clone() {
                let name = bound.symbols().get(symbol).name;
                let prints_as_the_name = data_text == name
                    || data_text.starts_with(&format!("{name}<"))
                    || data_text == format!("typeof {name}");
                if prints_as_the_name
                    && ast_namespace_container(bound, nodes, map, symbol).is_some()
                {
                    let meaning = SymbolFlags::TYPE | SymbolFlags::VALUE;
                    let resolved = bound.resolve_name(nodes, map, id, name, meaning);
                    // `qualname.rs`'s form: a raw id comparison.
                    if resolved != Some(symbol) {
                        report.cp6_unmerged += 1;
                        // The lines the merged comparison drops — upstream's
                        // `needsQualification` calls `getMergedSymbol` on the
                        // table hit (`symbolaccessibility.go:702`) before the
                        // identity test, so these are lines upstream does NOT
                        // qualify and `qualname.rs` counted anyway.
                        if resolved.map(|r| bound.merged_symbol(r))
                            == Some(bound.merged_symbol(symbol))
                        {
                            *report
                                .cp6_unmerged_lines
                                .entry(format!("{}  [{}]", want.text, case.name))
                                .or_default() += 1;
                        }
                    }
                    if needs_qualification(bound, nodes, map, symbol, name, id, meaning) {
                        report.cp6_ast_walk_at_risk += 1;
                    }
                }
            }

            if let Some((_, Some(symbol))) = named {
                let name = bound.symbols().get(symbol).name;
                if let Some((prefix, suffix)) = split_around_name(&printed, name) {
                    if is_gap {
                        report.gap_reached += 1;
                    }
                    let meaning = SymbolFlags::TYPE | SymbolFlags::VALUE;
                    let has_container = bound.symbols().get(symbol).parent.is_some();
                    match symbol_chain(bound, nodes, map, symbol, id, meaning, 0) {
                        Err(reason) => {
                            *report.declines.entry(reason.label()).or_default() += 1;
                        }
                        Ok(qualifier) => {
                            debug_assert!(has_container);
                            report.p_denominator += 1;
                            let forecast = format!("{prefix}{qualifier}{name}{suffix}");
                            // CP1 — upstream never qualifies a type parameter.
                            if bound
                                .symbols()
                                .get(symbol)
                                .flags
                                .intersects(SymbolFlags::TYPE_PARAMETER)
                            {
                                report.cp1_type_parameter_qualified += 1;
                            }
                            // CP2 — the chain must not be built past its stop
                            // condition.
                            if forecast != printed
                                && !needs_qualification(
                                    bound, nodes, map, symbol, name, id, meaning,
                                )
                            {
                                report.cp2_chain_past_stop += 1;
                            }

                            // The sub-families a positional refusal is priced
                            // against.
                            let parent =
                                bound.symbols().get(symbol).parent.map(|p| bound.merged_symbol(p));
                            let site_inside = parent.is_some_and(|p| {
                                bound
                                    .symbols()
                                    .get(p)
                                    .declarations
                                    .iter()
                                    .any(|&d| is_inside(nodes, id, d))
                            });
                            let in_lib = bound
                                .symbols()
                                .get(symbol)
                                .declarations
                                .iter()
                                .any(|&d| program.lib_files().iter().any(|f| f.contains(d)));
                            let flags = bound.symbols().get(symbol).flags;
                            let kind = if flags.intersects(SymbolFlags::ENUM_MEMBER) {
                                "enum member  "
                            } else if flags.intersects(SymbolFlags::ENUM) {
                                "enum         "
                            } else if flags.intersects(SymbolFlags::CLASS) {
                                "class        "
                            } else if flags.intersects(SymbolFlags::INTERFACE) {
                                "interface    "
                            } else if flags.intersects(SymbolFlags::TYPE_ALIAS) {
                                "type alias   "
                            } else if flags.intersects(SymbolFlags::MODULE) {
                                "namespace    "
                            } else {
                                "other        "
                            };
                            let family = format!(
                                "{} · {} · {} · {} segment(s) · {}",
                                kind,
                                if in_lib { "lib " } else { "user" },
                                if site_inside {
                                    "site INSIDE the container"
                                } else {
                                    "site outside            "
                                },
                                qualifier.matches('.').count(),
                                if suffix.is_empty() && prefix.is_empty() {
                                    "bare name     "
                                } else if prefix.is_empty() {
                                    "type arguments"
                                } else {
                                    "under `typeof`"
                                },
                            );

                            let outcome = if forecast == printed {
                                "NO-OP"
                            } else if is_right {
                                report.at_risk += 1;
                                *report
                                    .at_risk_lines
                                    .entry(format!("{printed}  ->  {forecast}  [{}]", case.name))
                                    .or_default() += 1;
                                *report.at_risk_cases.entry(case.name.clone()).or_default() += 1;
                                if case.name == CP5_CASE {
                                    report.cp5_parser_real_source_10 += 1;
                                }
                                "AT RISK"
                            } else if is_gap {
                                "GAP"
                            } else if forecast == wanted {
                                report.converts += 1;
                                *report
                                    .converts_lines
                                    .entry(format!(
                                        "want `{wanted}`, from `{printed}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                                *report.converts_cases.entry(case.name.clone()).or_default() += 1;
                                if CP4_CASES.contains(&short) {
                                    *report.cp4_hits.entry(case.name.clone()).or_default() += 1;
                                }
                                "CONVERTS"
                            } else {
                                report.churn += 1;
                                *report.churn_cases.entry(case.name.clone()).or_default() += 1;
                                *report
                                    .churn_lines
                                    .entry(format!(
                                        "want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                                "CHURN"
                            };
                            *report.families.entry((family, outcome)).or_default() += 1;
                            if CP4_CASES.contains(&short) {
                                *report
                                    .cp4_audit
                                    .entry(format!(
                                        "{outcome:<9} want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                            }
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            // LOOSE — token rewriting inside composite prints. An upper bound
            // on a FULLY qualifying node builder, not on the design above.
            // ---------------------------------------------------------------
            let mut loose = printed.clone();
            let mut changed = false;
            for token in name_tokens(&printed) {
                let Some(candidates) = namespaced.get(token) else { continue };
                let Some(&symbol) = candidates.iter().find(|&&symbol| {
                    needs_qualification(
                        bound,
                        nodes,
                        map,
                        symbol,
                        token,
                        id,
                        SymbolFlags::TYPE | SymbolFlags::VALUE,
                    )
                }) else {
                    continue;
                };
                let Ok(qualifier) = symbol_chain(
                    bound,
                    nodes,
                    map,
                    symbol,
                    id,
                    SymbolFlags::TYPE | SymbolFlags::VALUE,
                    0,
                ) else {
                    continue;
                };
                // Whole-token replacement only.
                let replaced = replace_token(&loose, token, &format!("{qualifier}{token}"));
                if replaced != loose {
                    loose = replaced;
                    changed = true;
                }
            }
            if changed {
                if is_right {
                    report.loose_at_risk += 1;
                    *report.loose_at_risk_cases.entry(case.name.clone()).or_default() += 1;
                    *report
                        .loose_at_risk_lines
                        .entry(format!("{printed}  ->  {loose}  [{}]", case.name))
                        .or_default() += 1;
                } else if !is_gap {
                    if loose == wanted {
                        report.loose_converts += 1;
                        *report
                            .loose_converts_lines
                            .entry(format!("want `{wanted}`, from `{printed}`  [{}]", case.name))
                            .or_default() += 1;
                    } else {
                        report.loose_churn += 1;
                    }
                }
            }
        }
    }
    Some(report)
}

/// Replace whole identifier tokens equal to `token`, leaving `A.token` and
/// `tokenSuffix` alone.
fn replace_token(text: &str, token: &str, with: &str) -> String {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '.';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(offset) = rest.find(token) {
        let before = &rest[..offset];
        let after = &rest[offset + token.len()..];
        let ok = before.chars().next_back().is_none_or(|c| !ident(c))
            && after.chars().next().is_none_or(|c| !ident(c));
        out.push_str(before);
        out.push_str(if ok { with } else { token });
        rest = after;
    }
    out.push_str(rest);
    out
}

#[allow(clippy::cast_precision_loss)]
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 { 0.0 } else { 100.0 * part as f64 / whole as f64 }
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
            println!("  -- total {total}, top-1 {n} = {:.1}%  ({key})", percent(**n, total));
        }
        None => println!("  -- total 0"),
    }
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]
fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");
    let mut report = Report::default();
    for partial in cases.par_iter().filter_map(measure).collect::<Vec<_>>() {
        report.merge(&partial);
    }

    println!("# qualnamep — design P re-sized against the compiler design W left behind\n");
    println!(
        "aligned lines:  right {}  gap {}  wrong {}   (total {})",
        report.right, report.gap, report.wrong, report.aligned
    );

    println!("\n## The three columns — STRICT (the design cycle 20b built)\n");
    println!("  CONVERTS   wrong today, P's text IS the baseline's      {:>7}", report.converts);
    println!("  CHURN      wrong today, P changes it, still not right   {:>7}", report.churn);
    println!("  AT RISK    RIGHT today, P changes the text — LOST       {:>7}", report.at_risk);
    println!(
        "  ---------  net if built                                {:>7}",
        report.converts as i64 - report.at_risk as i64
    );
    println!(
        "  ratio      gained per lost                              {:>7.2}",
        report.converts as f64 / report.at_risk.max(1) as f64
    );
    println!(
        "  denominator (strict gate holds and a chain was built)   {:>7}   at-risk share {:.2}%",
        report.p_denominator,
        percent(report.at_risk, report.p_denominator)
    );
    println!(
        "  cases:  converting {}, at risk {}",
        report.converts_cases.len(),
        report.at_risk_cases.len()
    );

    println!("\n## The three columns — LOOSE (a FULLY qualifying node builder; NOT this design)\n");
    println!("  CONVERTS  {:>7}", report.loose_converts);
    println!("  CHURN     {:>7}", report.loose_churn);
    println!(
        "  AT RISK   {:>7}   in {} cases",
        report.loose_at_risk,
        report.loose_at_risk_cases.len()
    );
    println!("  net       {:>7}", report.loose_converts as i64 - report.loose_at_risk as i64);

    println!("\n## Controls\n");
    println!(
        "  CP1  type parameters qualified                {:>7}   (expect 0, nodebuilderimpl.go:1069)",
        report.cp1_type_parameter_qualified
    );
    println!(
        "  CP2  chain built past its stop condition      {:>7}   (expect 0, nodebuilderimpl.go:1094)",
        report.cp2_chain_past_stop
    );
    println!(
        "  CP3  buckets {} + {} + {} + no-op = strict gate hits {}",
        report.converts, report.churn, report.at_risk, report.p_denominator
    );
    println!(
        "  CP4  conversions in §9.3's named residual cases {:>5}   (expect >= 13)",
        report.cp4_hits.values().sum::<usize>()
    );
    for (case, n) in &report.cp4_hits {
        println!("         {n:>5}  {case}");
    }
    println!(
        "  CP5  at-risk lines in {CP5_CASE} {:>4}   (expect 0; was -481)",
        report.cp5_parser_real_source_10
    );
    println!(
        "  CP6  at-risk under qualname.rs's AST-walk container {:>4}   (expect >= 36, it read 36 pre-W)",
        report.cp6_ast_walk_at_risk
    );
    println!(
        "       ... same, with qualname.rs's UNMERGED id comparison  {:>4}   (this is what 36 was)",
        report.cp6_unmerged
    );
    println!(
        "  CP7  right lines printing a dotted namespace name {:>6}   (expect > 0 — proves this tree HAS design W)",
        report.cp7_right_dotted
    );
    println!(
        "  --   gap lines the strict path reached        {:>7}   (expect 0 — `error` names no symbol)",
        report.gap_reached
    );

    println!("\n## Why a chain was NOT built\n");
    let total: usize = report.declines.values().sum();
    let mut declines: Vec<_> = report.declines.iter().collect();
    declines.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (reason, n) in declines {
        println!("  {n:>7}  {reason}");
    }
    println!("  -- total {total}");

    print_map(
        "CP6 — the lines getMergedSymbol drops (qualname.rs counted them)",
        &report.cp6_unmerged_lines,
        25,
    );
    print_map("CP4 — every P-visited line in §9.3's named cases", &report.cp4_audit, 40);
    print_map("CONVERTS — verbatim", &report.converts_lines, 30);
    print_map("CONVERTS — by case", &report.converts_cases, 20);
    print_map("AT RISK — verbatim", &report.at_risk_lines, 30);
    print_map("AT RISK — by case", &report.at_risk_cases, 20);
    print_map("CHURN — verbatim", &report.churn_lines, 20);
    print_map("CHURN — by case", &report.churn_cases, 20);
    print_map("LOOSE CONVERTS — verbatim", &report.loose_converts_lines, 20);
    print_map("LOOSE AT RISK — verbatim", &report.loose_at_risk_lines, 20);
    print_map("LOOSE AT RISK — by case", &report.loose_at_risk_cases, 20);

    println!("\n## The sub-family split — what a positional refusal would cost and save\n");
    let mut families: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    for ((family, outcome), n) in &report.families {
        *families.entry(family.clone()).or_default().entry(outcome).or_default() += n;
    }
    let mut ordered: Vec<_> = families.iter().collect();
    ordered.sort_by_key(|(_, cells)| {
        std::cmp::Reverse(
            cells.get("CONVERTS").copied().unwrap_or(0)
                + cells.get("AT RISK").copied().unwrap_or(0),
        )
    });
    println!(
        "  {:<78}  {:>8} {:>8} {:>7} {:>7}",
        "sub-family", "CONVERTS", "AT RISK", "CHURN", "NO-OP"
    );
    for (family, cells) in ordered {
        let get = |outcome| cells.get(outcome).copied().unwrap_or(0);
        if get("CONVERTS") + get("AT RISK") + get("CHURN") == 0 {
            continue;
        }
        println!(
            "  {family:<78}  {:>8} {:>8} {:>7} {:>7}",
            get("CONVERTS"),
            get("AT RISK"),
            get("CHURN"),
            get("NO-OP")
        );
    }
}
