//! Which **node kind** sits where a missing diagnostic should be?
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagnode
//! ```
//!
//! # The question this answers, and why nothing else could
//!
//! Every instrument in this workstream ranks **codes** — `diaggap`,
//! `diagslice`, `diagpair`, `diagcolumn`. §278's and §328's sweeps rank
//! **functions**. §335 is where the missing third axis cost something:
//!
//! > `class C implements I` fell between two *correct* predicates.
//! > `is_value_reference` excludes an `implements` name because only `extends`
//! > is a value; `check_type_reference_name` requires a `TypeReferenceNode`
//! > parent. Neither is wrong, and the position belongs to a node kind one
//! > rejects on purpose and the other never considered.
//! >
//! > **A gap between two correct predicates is invisible to both of them.**
//!
//! A two-line fixture stating its own expectation sat unconverted for fourteen
//! sessions because no ranking pointed at it. This ranks the **node kind at the
//! position of every missing line**, so a kind that no rule is asked about
//! shows up as a column rather than as a fixture nobody happened to read.
//!
//! `docs/architecture/checker-notes-diag2.md` §335, §336.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// One missing line: the node kind at its position, the code wanted there, and
/// the code this port emits at that exact position instead — `None` when it
/// emits nothing at all.
///
/// The third field is the **join** §369 said nothing performed. Every
/// instrument here ranks codes; this file ranks positions; the pair
/// `(position, wanted, emitted)` is what distinguishes *no rule looks here*
/// from *a rule looks and picks the wrong code*. §360 and §369 were both builds
/// spent on the second while assuming the first.
type Row = (String, u32, Option<u32>);

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().flat_map(measure).collect();

    let mut by_kind: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    // How many of a kind's missing lines sit at a position this port already
    // fills with a *different* code — the position is claimed, and porting the
    // wanted code adds a second diagnostic rather than converting anything.
    let mut claimed: BTreeMap<String, usize> = BTreeMap::new();
    for (kind, code, emitted) in rows {
        *by_kind.entry(kind.clone()).or_default().entry(code).or_default() += 1;
        if emitted.is_some() {
            *claimed.entry(kind).or_default() += 1;
        }
    }
    let mut ranked: Vec<(String, usize, BTreeMap<u32, usize>)> = by_kind
        .into_iter()
        .map(|(kind, codes)| {
            let total = codes.values().sum();
            (kind, total, codes)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    println!(
        "{:<34} {:>7} {:>8}   top codes wanted at this kind",
        "node kind at the position", "lines", "claimed"
    );
    for (kind, total, codes) in ranked.iter().take(28) {
        let claimed = claimed.get(kind).copied().unwrap_or(0);
        let mut top: Vec<(u32, usize)> = codes.iter().map(|(&c, &n)| (c, n)).collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let shown: Vec<String> = top.iter().take(4).map(|(c, n)| format!("TS{c}×{n}")).collect();
        let share = format!("{claimed}/{total}");
        println!("{kind:<34} {total:>7} {share:>8}   {}", shown.join("  "));
    }
}

fn measure(case: &CaseEntry) -> Vec<Row> {
    let empty = Vec::new();
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return empty;
    }
    let Ok(baseline) = case.expected_errors() else { return empty };
    let expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return empty;
    }
    let Ok(test) = case.load() else { return empty };
    let actual = tsr_conformance::diagnostics_suite::reported_for(&test);

    let mut got: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &actual {
        *got.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }
    let mut want: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &expected {
        *want.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }

    // One call per case: `(file, line, column, kind, parent kind)` for every
    // node the program parsed. The **parent** is what makes a gap legible —
    // §335's `implements` name is an `Identifier` whose parent is an
    // `ExpressionWithTypeArguments`, and the parent is the half no predicate
    // considered.
    let positions = tsr_conformance::diagnostics_suite::node_kinds_by_position_for(&test);
    let mut index: BTreeMap<(String, u32, u32), String> = BTreeMap::new();
    for (file, line, column, kind, parent) in positions {
        index.entry((file, line, column)).or_insert_with(|| match parent {
            Some(parent) => format!("{kind:?} in {parent:?}"),
            None => format!("{kind:?}"),
        });
    }

    let mut rows = Vec::new();
    for (key, n) in &want {
        if got.get(key).copied().unwrap_or(0) >= *n {
            continue;
        }
        let kind = index
            .get(&(key.0.clone(), key.1, key.2))
            .cloned()
            .unwrap_or_else(|| "<no node at position>".to_string());
        let emitted = got
            .keys()
            .find(|other| {
                (&other.0, other.1, other.2) == (&key.0, key.1, key.2) && other.3 != key.3
            })
            .map(|other| other.3);
        rows.push((kind, key.3, emitted));
    }
    rows
}
