//! `bd tsr-klm` — split the "callee has a type" call lines by **which gate**
//! turned them back. The registered prerequisite for revisiting
//! `docs/architecture/checker-notes-callres.md` §7's refusal: without it
//! "the 844 could be one change or five".
//!
//! `checker-notes-callres.md` §4 measured 5,006 lines whose blocking call has
//! a typed callee and which still gap. It did not measure *where* they stop.
//! This does, by reading `tsr_checker::calls::counters` **per blocking call
//! node** rather than per process:
//!
//! - pass A (parallel) finds every gap line's blocking call node and tallies
//!   lines per node, so the unit stays **assertion lines** — `bd tsr-cwz`'s
//!   correction, where a node count was quoted as a line count;
//! - pass B (**serial**, and it must be: `COUNTERS` is a process-global
//!   atomic, so a parallel delta is meaningless) rebuilds a **fresh
//!   `Checker` per node** and snapshots the counters across checking that one
//!   call. A fresh checker is the whole point — a shared one memoises, and a
//!   node already visited for an earlier line would read an all-zero delta
//!   and be attributed to nothing.
//!
//! Pass B visits only the cases pass A found candidates in, because serial
//! over 9,538 cases is minutes of lib loading for nothing.
//!
//! **C1 no longer reads 0, and the header carries the number rather than the
//! intention.** It was written expecting 0 and read **53** at `a57a04b`
//! (0.7% of 7,303), re-run on this instrument itself to confirm it is inherited
//! drift rather than something a later probe introduced. Eight builds have
//! landed since it was pinned, and a classified line that now *answers* is the
//! expected consequence of the checker improving under a predicate that admits
//! by call shape. **Do not quote a callgate figure without re-reading C1**: an
//! expected-0 control silently reading 53 is the shape `docs/conventions.md`
//! warns about, and the fix is to re-derive the admission predicate, not to
//! move the control's expected value.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0
//! violations). C2 buckets sum to classified. C3 an all-zero counter delta
//! means the fresh checker did **not** reach the call at all — printed as its
//! own bucket rather than folded into a gate, because it would otherwise
//! silently inflate whichever gate was listed first.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_binder::SymbolFlags;
use tsr_checker::calls::counters;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The blocking call node for a gap line, by the two routes
/// `checker-notes-callres.md` §3 describes.
fn blocking_call(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> Option<NodeId> {
    // Route 1: the line's own node is the call.
    if matches!(map.get(id), Some(Node::CallExpression(_) | Node::NewExpression(_))) {
        return Some(id);
    }
    // Route 2: the line is an identifier — a declaration name or a reference —
    // whose symbol's value declaration is initialised by a call.
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

/// Pass A's finding for one case: blocking node -> (lines, wants-any lines).
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
            // The admitted population: the callee already has a type. This is
            // §4's `callee HAS a type` bucket, recomputed here rather than
            // quoted.
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

/// Which gate a counter delta names, read in the order the code tests them
/// (`calls.rs`). `None` means the delta was empty — control C3.
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
    // The most specific bumped row wins: the funnel bumps the denominator
    // (`call expressions checked`) on every entry, so taking the first row
    // would attribute everything to it. Rows are declared in funnel order, so
    // the LAST bumped row is the deepest gate reached.
    delta.last().map(|(label, _)| label.trim())
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
    let admitted_lines: usize =
        found.iter().flat_map(|(_, c, _)| c.values()).map(|(lines, _)| lines).sum();
    let admitted_any: usize =
        found.iter().flat_map(|(_, c, _)| c.values()).map(|(_, any)| any).sum();
    let c1: usize = found.iter().map(|(_, _, not_gap)| not_gap).sum();
    println!("# callgate — bd tsr-klm: where the admitted call lines stop\n");
    println!("cases with candidates : {}", found.len());
    println!("admitted lines        : {admitted_lines}  (want-any {admitted_any})");
    println!("distinct blocking nodes: {}\n", by_name.values().map(|c| c.len()).sum::<usize>());

    // Pass B, SERIAL: process-global counters make any parallelism here wrong.
    let mut gates: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let mut unreached = 0usize;
    let mut case_of_gate: BTreeMap<&'static str, BTreeMap<String, usize>> = BTreeMap::new();
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
            // A FRESH checker per node: a shared one memoises, and the second
            // node would read an empty delta (control C3 would absorb it).
            let mut checker =
                tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
            let expression = match map.get(call) {
                Some(Node::CallExpression(node)) => Some(tsr_ast::Expression::CallExpression(node)),
                Some(Node::NewExpression(node)) => Some(tsr_ast::Expression::NewExpression(node)),
                _ => None,
            };
            let Some(expression) = expression else { continue };
            let before = counters::snapshot();
            let _ = checker.check_expression(expression);
            let after = counters::snapshot();
            match gate_of(&before, &after) {
                Some(gate) => {
                    let entry = gates.entry(gate).or_default();
                    entry.0 += lines;
                    entry.1 += any;
                    *case_of_gate.entry(gate).or_default().entry(case.name.clone()).or_default() +=
                        lines;
                }
                None => unreached += lines,
            }
        }
    }

    println!("## Where they stop — unit: ASSERTION LINES\n");
    let mut rows: Vec<_> = gates.iter().collect();
    rows.sort_by_key(|(_, (lines, _))| std::cmp::Reverse(*lines));
    let mut sum = 0;
    for (gate, (lines, any)) in rows {
        sum += lines;
        println!("  {lines:>6}  want-any {any:>5}  {gate}");
    }
    println!("  {unreached:>6}  {:>14}  C3 the fresh checker never reached the call", "");
    println!(
        "\n  C1 classified-but-not-gap: {c1}  (was 0 when pinned; 53 at `a57a04b` — see the header)"
    );
    println!("  C2 buckets sum {} vs admitted {admitted_lines}", sum + unreached);

    println!("\n## Top cases per gate\n");
    for (gate, cases) in &case_of_gate {
        let mut top: Vec<_> = cases.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let head: Vec<String> = top.iter().take(3).map(|(name, n)| format!("{name} {n}")).collect();
        println!("  {gate}\n      {}", head.join(" | "));
    }
}
