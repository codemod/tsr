//! Split `calls.rs`'s `undecidable_pair` gate by **which site** of
//! `docs/architecture/checker-notes-assign.md` §2 produced the `Unknown`.
//!
//! `cargo run -p tsr-conformance --example undecidable_split --release`
//!
//! `examples/callgate.rs` says *how many* admitted call lines stop because
//! `choose_overload` asked [`tsr_checker::Checker::relate_ternary`] about an
//! (argument, parameter) pair and got `Unknown`. It cannot say *why*, and the
//! why is the whole question: the six sites in §2's table are six different
//! items with six different costs, and the board cannot rank them from one
//! number. This instrument takes that split.
//!
//! # How the attribution works, and why it is exact here
//!
//! `tsr_checker::relater::reasons` records, per **top-level**
//! `relate_ternary` walk, the set of sites that fired, and keeps the mask of
//! the most recent walk that *returned* `Unknown`. `choose_overload` bails on
//! the first `Unknown` pair and `check_call_expression` then returns `error`
//! without relating anything else, so the deciding walk is the last
//! `Unknown`-returning walk of the whole node check — which is the mask this
//! reads. That claim is not assumed: control C4 counts `Unknown`-returning
//! walks per node, so a node where more than one walk went `Unknown` is
//! visible rather than silently attributed.
//!
//! A mask can name **several** sites: the Kleene combinators collect parts, so
//! one pair can be undecided for two reasons at once. Those are reported as
//! their own combination rows rather than being spread across the singles,
//! because spreading them would double-count lines and inflate every row.
//!
//! Passes are `callgate.rs`'s, for the same reasons: pass A parallel over the
//! corpus to find each gap line's blocking call node (unit = assertion lines,
//! `bd tsr-cwz`), pass B **serial** with a fresh `Checker` per node, because
//! the counters are process-global atomics.
//!
//! Controls. C1: every classified line answers `errorType` today. C2: buckets
//! sum to the classified total. C3: a node whose counter delta is empty was
//! never reached. C4: `Unknown`-returning walks per attributed node — 1 means
//! the attribution is unambiguous. C5: the reason mask of an attributed node
//! is non-empty; a zero mask would mean `undecidable_pair` fired without any
//! instrumented site producing the `Unknown`, i.e. a site this instrument does
//! not know about.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_binder::SymbolFlags;
use tsr_checker::calls::counters;
use tsr_checker::relater::reasons;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The blocking call node for a gap line — verbatim from `callgate.rs`, so the
/// two instruments classify the same population.
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

type Candidates = BTreeMap<NodeId, (usize, usize)>;

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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
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
            let wants_any = want.text.rsplit_once(" : ").is_some_and(|(_, answer)| answer == "any");
            let entry = candidates.entry(call).or_default();
            entry.0 += 1;
            entry.1 += usize::from(wants_any);
        }
    }
    (!candidates.is_empty()).then_some((case.name.clone(), candidates, not_gap))
}

/// Which gate a counter delta names. `callgate.rs`'s rule: rows are declared in
/// funnel order, so the deepest bumped row is the gate reached.
fn gate_of(before: &counters::Snapshot, after: &counters::Snapshot) -> Option<&'static str> {
    let mut delta: Vec<&'static str> = Vec::new();
    for ((label, a), (_, b)) in after.rows().into_iter().zip(before.rows()) {
        if a > b {
            delta.push(label);
        }
    }
    delta.last().map(|label| label.trim())
}

/// The label `calls.rs` gives the gate this instrument splits.
const UNDECIDABLE: &str = "a pair the relation cannot decide";

/// A reason mask rendered as its site names.
fn render(mask: u32) -> String {
    let labels = reasons::labels();
    let names: Vec<&str> =
        (0..reasons::SITES).filter(|i| mask & (1 << i) != 0).map(|i| labels[i]).collect();
    if names.is_empty() { "<EMPTY MASK — control C5>".to_string() } else { names.join(" + ") }
}

fn main() {
    counters::enable();
    reasons::enable();
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");

    let found: Vec<_> = cases.par_iter().filter_map(discover).collect();
    // C1 fires at 53 rather than at the 0 the fifth session measured, so name
    // the cases: a control is diagnosed, not tuned.
    let mut c1_cases: Vec<(usize, &str)> =
        found.iter().filter(|(_, _, n)| *n > 0).map(|(name, _, n)| (*n, name.as_str())).collect();
    c1_cases.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    if std::env::var_os("TSR_SPLIT_C1_ONLY").is_some() {
        println!("# C1 cases (classified as a gap, answered on re-ask)\n");
        for (n, name) in &c1_cases {
            println!("  {n:>4}  {name}");
        }
        return;
    }
    let by_name: BTreeMap<&str, &Candidates> =
        found.iter().map(|(name, c, _)| (name.as_str(), c)).collect();
    let admitted_lines: usize =
        found.iter().flat_map(|(_, c, _)| c.values()).map(|(lines, _)| lines).sum();
    let c1: usize = found.iter().map(|(_, _, not_gap)| not_gap).sum();
    println!("# undecidable_split — which of §2's six sites refuses the pair\n");
    println!("cases with candidates : {}", found.len());
    println!("admitted lines        : {admitted_lines}");

    // Pass B, SERIAL.
    let mut by_mask: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let mut by_site: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut cases_of_mask: BTreeMap<u32, BTreeMap<String, usize>> = BTreeMap::new();
    let mut walks_hist: BTreeMap<u64, usize> = BTreeMap::new();
    let mut gate_lines = 0usize;
    let mut gate_any = 0usize;
    let mut gate_nodes = 0usize;
    let mut unreached = 0usize;
    for case in &cases {
        let Some(candidates) = by_name.get(case.name.as_str()) else { continue };
        let Some(text) = case.expected_types() else { continue };
        let expected = types_baseline::parse(&text);
        let Ok(parsed) = case.load() else { continue };
        let arena = tsr_core::Arena::new();
        let (program, _, _) =
            types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
        let nodes = program.nodes();
        let map = program.node_map();
        let bound = program.binder();
        for (&call, &(lines, any)) in *candidates {
            let mut checker =
                tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
            let expression = match map.get(call) {
                Some(Node::CallExpression(node)) => Some(tsr_ast::Expression::CallExpression(node)),
                Some(Node::NewExpression(node)) => Some(tsr_ast::Expression::NewExpression(node)),
                _ => None,
            };
            let Some(expression) = expression else { continue };
            let before = counters::snapshot();
            let walks_before = reasons::unknown_walks();
            let _ = checker.check_expression(expression);
            let after = counters::snapshot();
            match gate_of(&before, &after) {
                None => unreached += lines,
                Some(gate) if gate == UNDECIDABLE => {
                    gate_lines += lines;
                    gate_any += any;
                    gate_nodes += 1;
                    *walks_hist.entry(reasons::unknown_walks() - walks_before).or_default() += 1;
                    let mask = reasons::last_unknown();
                    let entry = by_mask.entry(mask).or_default();
                    entry.0 += lines;
                    entry.1 += any;
                    *cases_of_mask
                        .entry(mask)
                        .or_default()
                        .entry(case.name.clone())
                        .or_default() += lines;
                    for site in 0..reasons::SITES {
                        if mask & (1 << site) != 0 {
                            let entry = by_site.entry(site).or_default();
                            entry.0 += lines;
                            entry.1 += any;
                        }
                    }
                }
                Some(_) => {}
            }
        }
    }

    println!("\n## The gate\n");
    println!("  {gate_lines} lines (want-any {gate_any}) across {gate_nodes} blocking nodes\n");

    println!("## By reason MASK — these partition the lines\n");
    let mut rows: Vec<_> = by_mask.iter().collect();
    rows.sort_by_key(|(_, (lines, _))| std::cmp::Reverse(*lines));
    let mut sum = 0;
    for (mask, (lines, any)) in rows {
        sum += lines;
        println!("  {lines:>5}  want-any {any:>4}  {}", render(*mask));
    }

    println!("\n## By SITE — a line appears in every site of its mask, so these OVERLAP\n");
    let labels = reasons::labels();
    let mut sites: Vec<_> = by_site.iter().collect();
    sites.sort_by_key(|(_, (lines, _))| std::cmp::Reverse(*lines));
    for (site, (lines, any)) in sites {
        println!("  {lines:>5}  want-any {any:>4}  {}", labels[*site]);
    }

    println!("\n## Top cases per mask\n");
    for (mask, cases) in &cases_of_mask {
        let mut top: Vec<_> = cases.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let head: Vec<String> = top.iter().take(4).map(|(n, c)| format!("{n} {c}")).collect();
        println!("  {}\n      {}", render(*mask), head.join(" | "));
    }

    println!("\n## Process-wide site totals (every walk, not just the deciding ones)\n");
    for (site, total) in reasons::totals().iter().enumerate() {
        println!("  {total:>9}  {}", labels[site]);
    }
    println!("\n## Flags seen on a side refused as `unported flag`\n");
    let names = [
        "ANY",
        "UNKNOWN",
        "UNDEFINED",
        "NULL",
        "VOID",
        "STRING",
        "NUMBER",
        "BIG_INT",
        "BOOLEAN",
        "ES_SYMBOL",
        "STRING_LITERAL",
        "NUMBER_LITERAL",
        "BIG_INT_LITERAL",
        "BOOLEAN_LITERAL",
        "UNIQUE_ES_SYMBOL",
        "ENUM_LITERAL",
        "ENUM",
        "NON_PRIMITIVE",
        "NEVER",
        "TYPE_PARAMETER",
        "OBJECT",
        "INDEX",
        "TEMPLATE_LITERAL",
        "STRING_MAPPING",
        "SUBSTITUTION",
        "INDEXED_ACCESS",
        "CONDITIONAL",
        "UNION",
        "INTERSECTION",
        "?29",
        "?30",
        "?31",
    ];
    let mut histogram: Vec<(u64, &str)> = reasons::flag_histogram()
        .iter()
        .zip(names)
        .filter(|(count, _)| **count > 0)
        .map(|(count, name)| (*count, name))
        .collect();
    histogram.sort_by_key(|(count, _)| std::cmp::Reverse(*count));
    for (count, name) in histogram {
        println!("  {count:>9}  {name}");
    }

    println!("\n## Controls\n");
    println!("  C1 classified-but-not-gap : {c1}  (expect 0)");
    println!("  C2 mask buckets sum {sum} vs gate {gate_lines}  (expect equal)");
    println!("  C3 never-reached lines    : {unreached}  (expect 0)");
    // Expectation, stated before the run: every attributed node reports >= 1,
    // and a node reporting exactly 1 is attributed unambiguously. A node
    // reporting more than 1 had another `Unknown` walk somewhere in its check
    // (argument checking, before the deciding pair); the deciding walk is still
    // the last one, because the gate returns immediately, but the row is
    // printed so the share resting on that argument is visible.
    println!("  C4 Unknown-returning walks per attributed node (expect >= 1; 1 = unambiguous):");
    for (walks, nodes) in &walks_hist {
        println!("        {walks} walk(s): {nodes} nodes");
    }
    println!(
        "  C5 empty-mask lines       : {}  (expect 0)",
        by_mask.get(&0).map_or(0, |(lines, _)| *lines)
    );
}
