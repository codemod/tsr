//! The 11,008 "fails on naming" wrong lines, as **exact pairs** — `bd tsr-jle`.
//!
//! # Why
//!
//! `wrongflip.rs` reports 11,008 wrong assertion lines (29.19% of the wrong
//! bucket) whose failure it classifies as *naming* rather than *typing*:
//! upstream prints a name and we print a structure, or the reverse. Its own
//! issue records the caveat that blocks quoting the figure:
//!
//! > `is_nominal`/`is_structural` are **syntactic tests on the printed
//! > string**. They cannot distinguish a type this port *has and cannot name*
//! > from one it *computed differently* and happened to print structurally. So
//! > 11,008 is an upper bound on the printer item and a lower bound on nothing.
//! > First thing to do is take a sample of ~50 and classify by hand.
//!
//! This is that step, done frequency-weighted rather than by an unweighted
//! sample of 50: a sample of 50 from a long tail tells you about the tail, and
//! what a *planner* needs is the head. The classifier's two arms are reproduced
//! here character for character from `wrongflip.rs` so the population is the
//! same one the 11,008 came from — a different predicate would be measuring a
//! different item and the comparison would be worthless.
//!
//! # What the output is for
//!
//! Each printed row is `want -> got`, with counts and the case that supplies
//! the most of them. The hand classification the issue asks for is then a
//! reading of the head rows, and it answers **one** question per row: if the
//! printer could name what we computed, would this line match? For a row like
//! `C -> { x: number; }` the answer depends on whether `{ x: number; }` is
//! actually `C`'s shape — which is why the rows carry an example expression.
//!
//! # Controls
//!
//! - **C1, frozen.** The naming total is compared against `wrongflip.rs`'s
//!   published 10,463 + 545 = 11,008 at `058b4a9`, a number in a committed
//!   document that no mutation of this file can move. It is a *different
//!   compiler* — the gradient has moved 61.09% → 63.34% since — so the
//!   comparison bounds drift, it does not assert equality.
//! - **C2, construction.** `ErrorLeak` and `AnyAnswer` are tested *before* the
//!   naming arms, exactly as in `wrongflip.rs`. A line where we print `any` is
//!   never a naming failure, and this bucket must be 0 inside the naming
//!   population by construction of the `if`-chain order.

use std::collections::HashMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// `wrongflip.rs:240`, reproduced verbatim.
fn is_nominal(printed: &str) -> bool {
    printed.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && !printed.contains('{')
        && !printed.contains('(')
        && !printed.contains("=>")
        && !printed.contains('[')
        && !printed.contains('|')
        && !printed.contains('&')
}

/// `wrongflip.rs:251`, reproduced verbatim.
fn is_structural(printed: &str) -> bool {
    printed.contains('{') || printed.contains("=>") || printed.contains('[')
}

struct Row {
    want: String,
    got: String,
    expr: String,
    case: String,
    /// Upstream names, we spell it out — the 10,463 leg.
    theirs_named: bool,
}

fn measure(case: &tsr_conformance::CaseEntry) -> Option<Vec<Row>> {
    if case.has_varied_types() || case.has_known_divergence() {
        return None;
    }
    let text = case.expected_types()?;
    let expected = types_baseline::parse(&text);
    if types_baseline::assertion_count(&expected) == 0 {
        return None;
    }
    let parsed = case.load().ok()?;
    let ours = types_producer::assertions_for_case(&parsed, &expected, false);

    let mut out = Vec::new();
    for (index, expected_file) in expected.iter().enumerate() {
        let Some(our_file) = ours.get(index) else { continue };
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            if want.text == got.line() {
                continue;
            }
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            // Wrong, not gap: `wrongflip`'s population.
            if got.type_string == "error" {
                continue;
            }
            // C2: the order is `wrongflip`'s. These two arms come first.
            if got.type_string.contains("error") || got.type_string == "any" {
                continue;
            }
            let theirs_named = is_nominal(want_type) && is_structural(&got.type_string);
            let ours_named = is_structural(want_type) && is_nominal(&got.type_string);
            if !theirs_named && !ours_named {
                continue;
            }
            out.push(Row {
                want: want_type.to_owned(),
                got: got.type_string.clone(),
                expr: got.text.clone(),
                case: case.name.clone(),
                theirs_named,
            });
        }
    }
    Some(out)
}

#[derive(Default)]
struct Pair {
    lines: usize,
    cases: HashMap<String, usize>,
    expr: String,
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("discovering cases");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).flatten().collect();

    let theirs = rows.iter().filter(|r| r.theirs_named).count();
    println!(
        "naming population {} = {} theirs-named/ours-structural + {} the reverse",
        rows.len(),
        theirs,
        rows.len() - theirs
    );
    println!("C1 frozen: wrongflip published 10,463 + 545 = 11,008 at 058b4a9 (a different tree)");

    // Where the population actually lives, per case. Printed before the pair
    // tables because a row that is one case is a different fact from a row
    // spread over five hundred (`docs/conventions.md`, concentration).
    let mut by_case: HashMap<String, usize> = HashMap::new();
    for row in &rows {
        *by_case.entry(row.case.clone()).or_default() += 1;
    }
    let mut ranked: Vec<(&String, &usize)> = by_case.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("  spread over {} cases; the largest five:", by_case.len());
    for (case, n) in ranked.iter().take(5) {
        println!("    {n:>6}  {case}");
    }

    for (label, want_named) in [("THEIRS NAMED, OURS STRUCTURAL", true), ("THE REVERSE", false)] {
        let mut pairs: HashMap<(String, String), Pair> = HashMap::new();
        let mut total = 0usize;
        for row in rows.iter().filter(|r| r.theirs_named == want_named) {
            total += 1;
            let pair = pairs.entry((row.want.clone(), row.got.clone())).or_default();
            pair.lines += 1;
            *pair.cases.entry(row.case.clone()).or_default() += 1;
            if pair.expr.is_empty() {
                pair.expr.clone_from(&row.expr);
            }
        }
        let mut ranked: Vec<(&(String, String), &Pair)> = pairs.iter().collect();
        ranked.sort_by(|a, b| b.1.lines.cmp(&a.1.lines).then_with(|| a.0.cmp(b.0)));
        println!("\n=== {label} — {total} lines in {} distinct pairs", pairs.len());
        let mut shown = 0usize;
        for ((want, got), pair) in ranked.iter().take(30) {
            let top = pair.cases.iter().max_by_key(|(_, n)| **n).map_or("", |(c, _)| c.as_str());
            shown += pair.lines;
            println!(
                "  {:>5}  {:>4} cases  {:<28} -> {:<44}  e.g. `{}` in {top}",
                pair.lines,
                pair.cases.len(),
                want,
                got,
                pair.expr
            );
        }
        println!("  (top 30 pairs cover {shown} of {total})");
    }
}
