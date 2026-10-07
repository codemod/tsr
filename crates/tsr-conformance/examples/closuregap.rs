//! The `any → undefined` W2 family, classified by `checker.go:11120`–`:11175`'s
//! own clauses — the counterfactual TASK.md staged before any flow surgery.
//!
//! # The population
//!
//! Wrong lines whose want is exactly `any` and whose got is exactly
//! `undefined` — 548 at the eighteenth build, 12 case-finishes, heads
//! `capturedLetConstInLoop*` and `jsxEsprimaFbTestSuite`.
//!
//! # What it classifies
//!
//! Per line, on the reference's identifier node:
//!
//! - **container split**: is the reference's control-flow container (nearest
//!   function-like ancestor, the file counting as one) different from the
//!   declaration's? Upstream's `isOuterVariable`.
//! - **initializer shape**: does the declaration carry an initializer?
//!   Upstream's `isNeverInitialized` reads `!initializer` (plus mutability
//!   and definite-assignment refinements this probe does not model — stated,
//!   so the buckets read as coarser than the table, never finer).
//!
//! The cross-tab decides which of upstream's arms the family actually sits
//! in, which is what the sketch-vs-table warning in TASK.md demands before
//! code.
//!
//! # Controls
//!
//! - **C1, arithmetic**: buckets sum to the matched population, printed.
//! - **C2, frozen**: the population must reproduce the W2 row's 548 within
//!   noise; a large delta means the selection drifted from `wrongflip`'s.

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    matched: usize,
    buckets: BTreeMap<&'static str, usize>,
    bucket_cases: BTreeMap<&'static str, HashMap<String, usize>>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.matched += other.matched;
        for (k, n) in &other.buckets {
            *self.buckets.entry(k).or_default() += n;
        }
        for (k, cases) in &other.bucket_cases {
            let mine = self.bucket_cases.entry(k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
    }
}

fn control_flow_container(nodes: &tsr_ast::NodeTable, id: NodeId) -> Option<NodeId> {
    let mut current = nodes.parent(id);
    while let Some(node) = current {
        match nodes.kind(node) {
            SyntaxKind::FunctionDeclaration
            | SyntaxKind::FunctionExpression
            | SyntaxKind::ArrowFunction
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::Constructor
            | SyntaxKind::SourceFile => return Some(node),
            _ => {}
        }
        current = nodes.parent(node);
    }
    None
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();

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
            if want_type != "any" || got.type_string != "undefined" {
                continue;
            }
            report.matched += 1;

            let id = line_ids[position];
            let bucket = (|| {
                let Some(Node::Identifier(identifier)) = map.get(id) else {
                    return "the line's node is not an identifier reference";
                };
                let Some(symbol) =
                    bound.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)
                else {
                    return "the name does not resolve";
                };
                let Some(declaration) = bound.symbols().get(symbol).value_declaration else {
                    return "no value declaration";
                };
                let has_initializer =
                    map.get(declaration).and_then(|n| n.initializer_id()).is_some();
                let reference_container = control_flow_container(nodes, id);
                let declaration_container = control_flow_container(nodes, declaration);
                let outer = reference_container != declaration_container;
                match (outer, has_initializer) {
                    (true, true) => "OUTER reference, declaration HAS initializer",
                    (true, false) => "OUTER reference, declaration has NO initializer",
                    (false, true) => "same container, declaration HAS initializer",
                    (false, false) => "same container, declaration has NO initializer",
                }
            })();
            *report.buckets.entry(bucket).or_default() += 1;
            *report
                .bucket_cases
                .entry(bucket)
                .or_default()
                .entry(case.name.clone())
                .or_default() += 1;
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

    println!("# closuregap — the `any → undefined` family by container and initializer\n");
    println!("matched lines: {} (C2: the W2 row reads 548)\n", report.matched);
    let mut rows: Vec<_> = report.buckets.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let total: usize = report.buckets.values().sum();
    for (bucket, n) in &rows {
        let empty = HashMap::new();
        let cases = report.bucket_cases.get(*bucket).unwrap_or(&empty);
        let (top, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
        println!(
            "  {n:>5}  {:>4} cases, top-1 {:>5.1}%  {bucket}\n         top: {top}",
            cases.len(),
            *top_n as f64 / (**n).max(1) as f64 * 100.0,
        );
    }
    println!("\n  C1 arithmetic: buckets sum {total}, matched {}", report.matched);
}
