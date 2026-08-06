//! Sizing `autoArrayType` and the evolving-array machinery, corpus-wide.
//!
//! `cargo run --release -p tsr-conformance --example evolvearray`
//!
//! # The question, and why the headline case does not answer it
//!
//! `compiler/largeControlFlowGraph` holds 20,001 unmatched assertion lines —
//! 4.18 gradient points, the largest single-case residual in the corpus — and
//! the substitution is `any -> never[]` on a variable declared `const data =
//! [];`. That reads as the evolving-array machinery `crates/tsr-checker/src/flow.rs`
//! documents as a deliberate omission.
//!
//! **It is mostly not.** The case's baseline sits under
//! `error TS2563: The containing function or module body is too large for
//! control flow analysis`, and upstream's bailout is explicit:
//!
//! ```text
//! // vendor/typescript-go/internal/checker/flow.go:120-125
//! if f.depth == 2000 {
//!     c.flowAnalysisDisabled = true
//!     c.reportFlowControlError(f.reference)
//!     return FlowType{t: c.errorType}
//! }
//! // and flow.go:81-83
//! if c.flowAnalysisDisabled { return c.errorType }
//! ```
//!
//! `c.errorType` is `c.newIntrinsicType(TypeFlagsAny, "error")`
//! (`checker.go:979`), and per ADR-0038 the node builder renders anything
//! carrying `TypeFlagsAny` as the `any` keyword. So the 20,000 `>data : any` /
//! `>data[0] : any` lines in that case are **upstream's `errorType`, printed
//! `any`** — ADR-0038's ceiling, not an evolving array. TS2563 appears in
//! exactly **one** baseline in the corpus.
//!
//! What is left of the item, and what this probe measures, is the *general*
//! one: a variable initialised with an empty array literal gets `autoArrayType`
//! (`checker.go:1360`), and `x.push(e)` widens its element type through an
//! `ARRAY_MUTATION` flow node. That is corpus-wide work and it has never been
//! sized.
//!
//! # Sizing the conversion, not the population
//!
//! `docs/conventions.md`: a row is a **ceiling**, and an item sized at 1,784
//! lines converted 362 because a name resolving is necessary and not sufficient
//! for the line to match. So this probe does not stop at "lines that mention an
//! auto-array variable". It splits them by **upstream's own answer**, because
//! that is what says which mechanism each line needs:
//!
//! - upstream says `any[]` — `autoArrayType` alone answers it. The cheap half.
//! - upstream says `any` — the element, or an `errorType` that printed `any`.
//! - upstream says a **concrete** `T[]` — the full evolving machinery
//!   (`addEvolvingArrayElementType` + `finalizeEvolvingArrayType`) is required;
//!   `autoArrayType` alone would answer `any[]` and be **wrong**.
//! - upstream says `never[]` — we already agree, or would break it.
//!
//! Only the first two are conversions from `autoArrayType`. The third is a
//! *negative* for the cheap fix, and it is counted separately and printed,
//! because `docs/conventions.md` records two designs refused at 2.1 and 2.5
//! wrong-per-right where the damage was in lines the slice never targeted.
//!
//! # Controls, printed unconditionally
//!
//! - **C1 — the headline case has exactly one auto-array declaration.**
//!   `largeControlFlowGraph.ts` is `const data = [];` followed by 10,000
//!   `data[0] = 0;`. There is exactly **one** empty-array-initialised variable
//!   in it, and that is true of the *source file* before any code here was
//!   written. 0 or 2 means the finder is broken. Pinned by the subject.
//! - **C2 — an annotated declaration is not auto-array.** `const x: number[] =
//!   []` takes its annotation, so the count of collected symbols whose
//!   declaration carries a type annotation is **0**: annotated declarations are
//!   excluded by a positive test. The mirror — how many annotated
//!   empty-array declarations were seen and rejected — is printed beside it, so
//!   the pair pins the filter rather than one arm of it.
//! - **C3 — a collected line's node resolves to a collected symbol.** Every
//!   line attributed here is attributed through a resolved `SymbolId`, never
//!   through a name match, so the count attributed to a symbol not in the set
//!   is **0**. It reads non-zero if the attribution ever falls back to text.
//! - **A1 — the status split partitions the collected lines.** Arithmetic.
//!   `Status::Unaligned` is a real arm with a positive test, not a default.
//!
//! # `errorType`, never `anyType`
//!
//! This item is the one where that rule is easiest to bend by accident, because
//! upstream's correct answer genuinely *is* `any`. The distinction this probe
//! keeps is between an `any[]` upstream **computed** (`autoArrayType`, a real
//! type with a real construction) and an `any` upstream **fell back to**
//! (`errorType` under TS2563). They are counted in different buckets and the
//! second is reported as unreachable rather than as work.

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// What we do with a line today.
///
/// `Unaligned` carries a positive test — the walker did not reproduce
/// upstream's expression text — so control A1 is over a genuine partition.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Status {
    Right,
    Gap,
    Wrong,
    Unaligned,
}

impl Status {
    const fn label(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Gap => "gap (we said `error`)",
            Self::Wrong => "wrong",
            Self::Unaligned => "unaligned (walker text)",
        }
    }
}

/// Which mechanism a line needs, read off **upstream's own answer**.
///
/// Every arm is a positive test on the baseline's right-hand side.
/// `Other` is the default and is deliberately the least informative label —
/// `docs/conventions.md`, *"never let the semantically loaded label be the
/// default arm"*.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Needs {
    /// `any[]` — `autoArrayType` alone answers it.
    AutoArrayType,
    /// `any` — the element of an auto array, or an `errorType` that printed
    /// `any`. Cheap only if it is the former.
    BareAny,
    /// `never[]` — upstream did **not** apply `autoArrayType` here.
    NeverArray,
    /// A concrete `T[]`. Needs the full evolving machinery; `autoArrayType`
    /// alone would answer `any[]` and be wrong.
    ConcreteArray,
    /// Anything else.
    Other,
}

impl Needs {
    const fn label(self) -> &'static str {
        match self {
            Self::AutoArrayType => "any[]   — autoArrayType alone",
            Self::BareAny => "any     — element, or errorType printed as any",
            Self::NeverArray => "never[] — upstream did NOT auto-array here",
            Self::ConcreteArray => "T[]     — needs the full evolving machinery",
            Self::Other => "other",
        }
    }
}

/// Would `autoArrayType` alone answer this line?
///
/// One predicate, called from both roll-ups, so the per-case column and the
/// corpus total cannot drift apart — `docs/conventions.md` records two
/// roll-ups of the same quantity disagreeing because each computed it.
///
/// The `flow_disabled` arm is the load-bearing one: in a case where upstream
/// **disabled** control flow analysis (TS2563), an `any` on a reference is
/// `c.errorType` (`flow.go:81`) and not an auto array, so it is ADR-0038's
/// ceiling. The declaration name is exempt because it is typed before any flow
/// analysis happens.
fn reachable(need: Needs, site: Site, flow_disabled: bool) -> bool {
    match need {
        Needs::AutoArrayType => true,
        Needs::BareAny => !flow_disabled || site == Site::DeclarationName,
        Needs::NeverArray | Needs::ConcreteArray | Needs::Other => false,
    }
}

fn needs(upstream: &str) -> Needs {
    if upstream == "any[]" {
        Needs::AutoArrayType
    } else if upstream == "any" {
        Needs::BareAny
    } else if upstream == "never[]" {
        Needs::NeverArray
    } else if upstream.ends_with("[]") {
        Needs::ConcreteArray
    } else {
        Needs::Other
    }
}

/// Where the line sits relative to the auto-array variable. Positive tests
/// throughout; `Other` is the default.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Site {
    /// The `x` of `const x = []`.
    DeclarationName,
    /// The `[]` of `const x = []`.
    Initialiser,
    /// A reference to `x` somewhere else.
    Reference,
    /// `x[0]`, with `x` the auto-array variable.
    ElementAccess,
    /// `x.push`, `x.length`.
    PropertyAccess,
    Other,
}

impl Site {
    const fn label(self) -> &'static str {
        match self {
            Self::DeclarationName => "the `x` of `const x = []`",
            Self::Initialiser => "the `[]` of `const x = []`",
            Self::Reference => "a reference to `x`",
            Self::ElementAccess => "`x[i]`",
            Self::PropertyAccess => "`x.p`",
            Self::Other => "other",
        }
    }
}

#[derive(Default)]
struct CaseReport {
    name: String,
    /// The case's own `.errors.txt` carries TS2563, so upstream **disabled**
    /// control flow analysis in it and every later `any` on a reference is
    /// `c.errorType` (`flow.go:81`), not an auto array. Read from the baseline
    /// rather than assumed, and it is the positive test that separates the
    /// reachable lines from ADR-0038's ceiling.
    flow_disabled: bool,
    expected: usize,
    matched: usize,
    /// Collected lines keyed by every axis, so no roll-up is inferred from
    /// another.
    lines: HashMap<(Site, Needs, Status), usize>,
    /// The exact `upstream -> ours` pairs, for the spellability read.
    pairs: HashMap<(Needs, String, String), usize>,
    /// Auto-array symbols found, for the concentration check.
    symbols: usize,
    /// Total collected lines in this case, for `finishes`.
    collected: usize,
    /// Collected lines that are **not** already right — the honest numerator.
    convertible: usize,
    c1_headline_declarations: usize,
    c2_annotated_collected: usize,
    c2_annotated_rejected: usize,
    c3_unknown_symbol: usize,
}

fn check_classifier() {
    // ORDER. `any[]` ends with `[]` and would fall into ConcreteArray if the
    // generic array arm were tested first — collapsing the cheap half into the
    // expensive one and inverting the whole ranking.
    assert_eq!(needs("any[]"), Needs::AutoArrayType, "any[] must be tested before the T[] arm");
    assert_eq!(needs("never[]"), Needs::NeverArray, "never[] must be tested before the T[] arm");
    assert_eq!(needs("number[]"), Needs::ConcreteArray);
    assert_eq!(needs("any"), Needs::BareAny);
    assert_eq!(needs("number"), Needs::Other);
    // The distinction the `errorType, never anyType` rule turns on: a computed
    // `any[]` and a fallen-back-to `any` are different buckets, never merged.
    assert_ne!(needs("any[]"), needs("any"), "computed autoArrayType is not a bare any");
    // REACHABILITY. The `any` on a reference inside a TS2563 case is upstream's
    // errorType, not an auto array; the declaration name is typed before any
    // flow analysis and is therefore still reachable. Mutation N2 removes the
    // distinction and the item reads 21,775 lines instead of 1,775.
    assert!(reachable(Needs::AutoArrayType, Site::Reference, true), "any[] never depends on flow");
    assert!(!reachable(Needs::BareAny, Site::Reference, true), "a TS2563 `any` is errorType");
    assert!(reachable(Needs::BareAny, Site::Reference, false), "an ordinary `any` is the element");
    assert!(
        reachable(Needs::BareAny, Site::DeclarationName, true),
        "a declaration name is typed before flow analysis runs"
    );
    assert!(!reachable(Needs::ConcreteArray, Site::Reference, false), "T[] needs more than auto");
}

fn main() {
    check_classifier();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let reports: Vec<CaseReport> = cases.par_iter().filter_map(measure).collect();
    report(&reports);
}

/// Is this declaration `const/let/var x = []` with no annotation?
///
/// Positive on both halves: the initialiser must be an `ArrayLiteralExpression`
/// with **zero** elements, and the declaration must carry **no** type
/// annotation. `const x: number[] = []` takes its annotation upstream and is
/// rejected here — control C2 counts both sides of that.
fn auto_array_declaration(map: &tsr_ast::NodeMap<'_>, declaration: NodeId) -> Option<bool> {
    let Some(Node::VariableDeclaration(node)) = map.get(declaration) else { return None };
    let initialiser = node.initializer.as_ref()?;
    let id = initialiser.node_id()?;
    let Some(Node::ArrayLiteralExpression(array)) = map.get(id) else { return None };
    if !array.elements.is_empty() {
        return None;
    }
    // `Some(true)` = auto-array. `Some(false)` = an empty-array initialiser
    // that is annotated, which is control C2's mirror.
    Some(node.r#type.is_none())
}

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<CaseReport> {
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

    let mut report = CaseReport {
        name: case.name.clone(),
        flow_disabled: case
            .expected_errors()
            .ok()
            .flatten()
            .is_some_and(|errors| errors.contains("TS2563")),
        ..CaseReport::default()
    };

    // Every symbol whose value declaration is `x = []` with no annotation.
    let mut auto: HashSet<SymbolId> = HashSet::new();
    let mut auto_declaration: HashMap<NodeId, SymbolId> = HashMap::new();
    for (symbol, data) in bound.symbols().iter() {
        let Some(declaration) = data.value_declaration else { continue };
        match auto_array_declaration(map, declaration) {
            Some(true) => {
                auto.insert(symbol);
                auto_declaration.insert(declaration, symbol);
            }
            Some(false) => report.c2_annotated_rejected += 1,
            None => {}
        }
    }
    report.symbols = auto.len();
    if case.name == "compiler/largeControlFlowGraph" {
        report.c1_headline_declarations = auto.len();
    }
    if auto.is_empty() {
        // Still counted for the residual, so `finishes` has a denominator.
        for expected_file in &expected {
            report.expected += expected_file.assertions.len();
        }
        for (index, expected_file) in expected.iter().enumerate() {
            let Some(our_file) = ours.get(index) else { continue };
            for (position, want) in expected_file.assertions.iter().enumerate() {
                if our_file.get(position).is_some_and(|got| got.line() == want.text) {
                    report.matched += 1;
                }
            }
        }
        return Some(report);
    }

    for (index, expected_file) in expected.iter().enumerate() {
        report.expected += expected_file.assertions.len();
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            let aligned = want.text.strip_prefix(&format!("{} : ", got.text));
            if aligned.is_some_and(|w| w == got.type_string) {
                report.matched += 1;
            }
            let id = line_ids[position];
            let Some((site, symbol)) = site_of(bound, nodes, map, id, &auto, &auto_declaration)
            else {
                continue;
            };
            // C3: attribution is through a resolved `SymbolId`, never a name.
            if !auto.contains(&symbol) {
                report.c3_unknown_symbol += 1;
                continue;
            }
            if map.get(bound.symbols().get(symbol).value_declaration.unwrap_or(id)).is_some_and(
                |node| matches!(node, Node::VariableDeclaration(v) if v.r#type.is_some()),
            ) {
                report.c2_annotated_collected += 1;
            }
            report.collected += 1;
            let Some(upstream) = aligned else {
                *report.lines.entry((site, Needs::Other, Status::Unaligned)).or_default() += 1;
                report.convertible += 1;
                continue;
            };
            let need = needs(upstream);
            let status = if upstream == got.type_string {
                Status::Right
            } else if got.type_string == "error" {
                report.convertible += 1;
                Status::Gap
            } else {
                report.convertible += 1;
                Status::Wrong
            };
            *report.lines.entry((site, need, status)).or_default() += 1;
            if status != Status::Right {
                *report
                    .pairs
                    .entry((need, upstream.to_string(), got.type_string.clone()))
                    .or_default() += 1;
            }
        }
    }
    Some(report)
}

/// Where this line sits relative to an auto-array variable, and which one.
fn site_of(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
    auto: &HashSet<SymbolId>,
    auto_declaration: &HashMap<NodeId, SymbolId>,
) -> Option<(Site, SymbolId)> {
    // The `[]` initialiser, and the `x` name: both are children of a collected
    // declaration, so they are found by the parent rather than by resolution.
    if let Some(parent) = nodes.parent(id)
        && let Some(&symbol) = auto_declaration.get(&parent)
    {
        return Some(match nodes.kind(id) {
            SyntaxKind::ArrayLiteralExpression => (Site::Initialiser, symbol),
            SyntaxKind::Identifier => (Site::DeclarationName, symbol),
            _ => (Site::Other, symbol),
        });
    }
    // A reference, or an access whose receiver is one.
    let resolve = |node: NodeId| -> Option<SymbolId> {
        let Some(Node::Identifier(identifier)) = map.get(node) else { return None };
        bound
            .resolve_name(nodes, map, node, identifier.text, SymbolFlags::VALUE)
            .filter(|symbol| auto.contains(symbol))
    };
    if let Some(symbol) = resolve(id) {
        return Some((Site::Reference, symbol));
    }
    match map.get(id) {
        Some(Node::ElementAccessExpression(node)) => node
            .expression
            .as_ref()
            .and_then(tsr_ast::Expression::node_id)
            .and_then(resolve)
            .map(|symbol| (Site::ElementAccess, symbol)),
        Some(Node::PropertyAccessExpression(node)) => node
            .expression
            .as_ref()
            .and_then(tsr_ast::Expression::node_id)
            .and_then(resolve)
            .map(|symbol| (Site::PropertyAccess, symbol)),
        _ => None,
    }
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

fn delta(left: usize, right: usize) -> i128 {
    i128::try_from(left).unwrap_or_default() - i128::try_from(right).unwrap_or_default()
}

#[allow(clippy::too_many_lines)]
fn report(reports: &[CaseReport]) {
    let expected: usize = reports.iter().map(|r| r.expected).sum();
    let matched: usize = reports.iter().map(|r| r.matched).sum();
    let collected: usize = reports.iter().map(|r| r.collected).sum();
    let convertible: usize = reports.iter().map(|r| r.convertible).sum();
    let symbols: usize = reports.iter().map(|r| r.symbols).sum();
    let with_symbols = reports.iter().filter(|r| r.symbols > 0).count();

    println!("# evolvearray — sizing autoArrayType and the evolving-array machinery\n");
    println!("cases judged:                {}", reports.len());
    println!("assertion lines upstream:    {expected}");
    println!("  exactly right:             {matched} ({:.2}%)", pct(matched, expected));
    println!("\ncases with >=1 `x = []`:     {with_symbols}");
    println!("auto-array symbols:          {symbols}");
    println!(
        "lines touching one:          {collected} ({:.3}% of the denominator)",
        pct(collected, expected)
    );
    println!(
        "  of those, NOT already right: {convertible} ({:.3} gradient points if every one converted)",
        pct(convertible, expected)
    );

    // Roll up.
    let mut by_key: BTreeMap<(Site, Needs, Status), usize> = BTreeMap::new();
    for case in reports {
        for (key, n) in &case.lines {
            *by_key.entry(*key).or_default() += n;
        }
    }

    println!("\n## What upstream actually answers on these lines (unit: assertion lines)\n");
    let mut by_needs: BTreeMap<Needs, BTreeMap<Status, usize>> = BTreeMap::new();
    for ((_, need, status), n) in &by_key {
        *by_needs.entry(*need).or_default().entry(*status).or_default() += n;
    }
    println!(
        "{:<48} {:>8} {:>8} {:>21} {:>10} {:>23}",
        "upstream's answer",
        "lines",
        Status::Right.label(),
        Status::Gap.label(),
        Status::Wrong.label(),
        Status::Unaligned.label()
    );
    for (need, split) in &by_needs {
        let total: usize = split.values().sum();
        println!(
            "{:<48} {:>8} {:>8} {:>21} {:>10} {:>23}",
            need.label(),
            total,
            split.get(&Status::Right).copied().unwrap_or_default(),
            split.get(&Status::Gap).copied().unwrap_or_default(),
            split.get(&Status::Wrong).copied().unwrap_or_default(),
            split.get(&Status::Unaligned).copied().unwrap_or_default(),
        );
    }

    println!("\n## Where the line sits (unit: assertion lines)\n");
    let mut by_site: BTreeMap<Site, (usize, usize)> = BTreeMap::new();
    for ((site, _, status), n) in &by_key {
        let entry = by_site.entry(*site).or_default();
        entry.0 += n;
        if *status != Status::Right {
            entry.1 += n;
        }
    }
    println!("{:<36} {:>8} {:>14}", "site", "lines", "not yet right");
    for (site, (total, wrong)) in &by_site {
        println!("{:<36} {:>8} {:>14}", site.label(), total, wrong);
    }

    println!("\n## Concentration — the cases holding these lines\n");
    let mut per_case: Vec<(&str, usize, usize, usize)> = reports
        .iter()
        .filter(|r| r.collected > 0)
        .map(|r| (r.name.as_str(), r.collected, r.convertible, r.expected - r.matched))
        .collect();
    per_case.sort_by_key(|(name, _, convertible, _)| (std::cmp::Reverse(*convertible), *name));
    println!("{:>10} {:>13} {:>12}  case", "collected", "convertible", "residual");
    for (name, collected, convertible, residual) in per_case.iter().take(15) {
        println!("{collected:>10} {convertible:>13} {residual:>12}  {name}");
    }
    let top1 = per_case.first().map_or(0, |(_, _, c, _)| *c);
    let top10: usize = per_case.iter().take(10).map(|(_, _, c, _)| c).sum();
    println!(
        "\ncases holding a convertible line: {}   top-1 {:.1}%   top-10 {:.1}%",
        per_case.iter().filter(|(_, _, c, _)| *c > 0).count(),
        pct(top1, convertible),
        pct(top10, convertible),
    );

    // The direct bucket: cases this item would FINISH.
    // The reachable set: everything `autoArrayType` alone would answer, with
    // the TS2563 cases' post-bailout `any`s removed because those are upstream's
    // `errorType` and ADR-0038 refuses to render ours as `any`.
    println!("\n## REACHABLE — what autoArrayType alone would actually answer\n");
    let mut reach_lines = 0usize;
    let mut reach_by_need: BTreeMap<Needs, usize> = BTreeMap::new();
    let mut unreachable = 0usize;
    let mut at_risk = 0usize;
    let mut reach_per_case: Vec<(&str, usize)> = Vec::new();
    for case in reports {
        let mut here = 0usize;
        for ((site, need, status), n) in &case.lines {
            if *status == Status::Right {
                continue;
            }
            if reachable(*need, *site, case.flow_disabled) {
                here += n;
            }
        }
        if here > 0 {
            reach_per_case.push((case.name.as_str(), here));
        }
    }
    for case in reports {
        for ((site, need, status), n) in &case.lines {
            // A currently-right line the fix would touch. The `[]` literal keeps
            // its own `never[]` type upstream (`>[] : never[]` beside
            // `>data : any[]`), so the initialiser site is out of scope by
            // design and is the one place `Right` lines live.
            if *status == Status::Right && *site != Site::Initialiser {
                at_risk += n;
            }
            if *status == Status::Right {
                continue;
            }
            if *need == Needs::BareAny && case.flow_disabled && *site != Site::DeclarationName {
                unreachable += n;
                continue;
            }
            if !reachable(*need, *site, case.flow_disabled) {
                continue;
            }
            *reach_by_need.entry(*need).or_default() += n;
            reach_lines += n;
        }
    }
    for (need, n) in &reach_by_need {
        println!("  {:<48} {n:>7}", need.label());
    }
    println!(
        "  {:<48} {reach_lines:>7}  ({:.3} gradient points)",
        "REACHABLE TOTAL",
        pct(reach_lines, expected)
    );
    println!(
        "  {:<48} {unreachable:>7}  ({:.3} points) — upstream's errorType under TS2563, ADR-0038",
        "UNREACHABLE (flow analysis disabled)",
        pct(unreachable, expected)
    );
    println!(
        "  {:<48} {at_risk:>7}  — currently-right lines at any site the fix touches",
        "AT RISK (E2's numerator)"
    );
    reach_per_case.sort_by_key(|(name, n)| (std::cmp::Reverse(*n), *name));
    println!("\n  Reachable lines by case — E3's disclosure, printed unconditionally:");
    for (name, n) in reach_per_case.iter().take(12) {
        println!("      {n:>6}  ({:>5.1}%)  {name}", pct(*n, reach_lines));
    }
    println!(
        "      {} cases hold a reachable line; top-1 {:.1}%, top-5 {:.1}%",
        reach_per_case.len(),
        pct(reach_per_case.first().map_or(0, |(_, n)| *n), reach_lines),
        pct(reach_per_case.iter().take(5).map(|(_, n)| n).sum::<usize>(), reach_lines)
    );

    println!("\n## The direct bucket — cases finished if every collected line converted\n");
    let finishes = per_case
        .iter()
        .filter(|(_, _, convertible, residual)| *convertible > 0 && convertible == residual)
        .count();
    println!("  cases where the collected lines ARE the entire remaining residual: {finishes}");
    let reach_finishes = reports
        .iter()
        .filter(|r| !r.flow_disabled)
        .filter(|r| r.convertible > 0 && r.convertible == r.expected - r.matched)
        .count();
    println!("  the same, over REACHABLE cases only (TS2563 cases excluded): {reach_finishes}");
    println!(
        "  (upper bound: assumes complete closure. `checker-notes-rank.md` §8 records the one"
    );
    println!("   measured analogue delivering 6.9% of touched cases against a predicted 10-25%.)");

    println!("\n## Exact substitutions, by what upstream needs\n");
    let mut pairs: BTreeMap<(Needs, String, String), usize> = BTreeMap::new();
    for case in reports {
        for (key, n) in &case.pairs {
            *pairs.entry(key.clone()).or_default() += n;
        }
    }
    for need in [Needs::AutoArrayType, Needs::BareAny, Needs::ConcreteArray, Needs::NeverArray] {
        let mut mine: Vec<(&String, &String, usize)> = pairs
            .iter()
            .filter(|((n, _, _), _)| *n == need)
            .map(|((_, want, got), n)| (want, got, *n))
            .collect();
        if mine.is_empty() {
            continue;
        }
        mine.sort_by_key(|(want, _, n)| (std::cmp::Reverse(*n), (*want).clone()));
        println!("  {}", need.label());
        for (want, got, n) in mine.iter().take(6) {
            println!("      {n:>7}  upstream `{want}`  ours `{got}`");
        }
    }

    println!("\n## CONTROLS\n");
    let c1: usize = reports.iter().map(|r| r.c1_headline_declarations).sum();
    let c2_collected: usize = reports.iter().map(|r| r.c2_annotated_collected).sum();
    let c2_rejected: usize = reports.iter().map(|r| r.c2_annotated_rejected).sum();
    let c3: usize = reports.iter().map(|r| r.c3_unknown_symbol).sum();
    println!(
        "  C1  auto-array declarations in compiler/largeControlFlowGraph = {c1} (must be 1: the file is one `const data = [];`)"
    );
    println!(
        "  C2  annotated declarations COLLECTED             = {c2_collected} (must be 0: `const x: number[] = []` takes its annotation)"
    );
    println!(
        "  C2' annotated empty-array declarations rejected  = {c2_rejected} (the mirror; pins the filter rather than one arm)"
    );
    println!(
        "  C3  a line attributed to an uncollected symbol   = {c3} (must be 0: attribution is by SymbolId, never by name)"
    );
    let split_total: usize = by_key.values().sum();
    println!(
        "\n  A1  status split - collected lines               = {} (must be 0)",
        delta(split_total, collected)
    );
}
