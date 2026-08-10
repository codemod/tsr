//! Extra diagnostics that sit at a position the baseline **also** records —
//! a second diagnostic where upstream emits one.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagdup
//! ```
//!
//! # Why this shape needs its own view
//!
//! `extraonly` lists every extra line and ranks nothing; `diagmissing` and
//! `diagreach` look at the missing side and cannot see an extra at all. A
//! **duplicate at a right position** is invisible to all three: the case has
//! the diagnostic upstream wants, at the column upstream wants, and fails
//! anyway because a second rule spoke at the same place.
//!
//! §858 found one by reading `extraonly` line by line —
//! `class C { async constructor() {} }` emitted TS1089 *and* TS1042 at column 3,
//! because this port's TS1042 rule lives outside `checkGrammarModifiers`'s
//! chain and restates its exclusions by hand. Upstream cannot produce that pair
//! at all: every arm of that chain ends in `return`, so a node gets at most one
//! grammar diagnostic.
//!
//! That is a **class**, not an incident — this port has ported several members
//! of that chain into rules of their own — and it is the class this view
//! enumerates.
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .expect("sizing the corpus thread pool");
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Vec<String>)> = cases.par_iter().filter_map(measure).collect();
    let (mut lines, mut duplicates) = (0, 0);
    for (name, dups) in &rows {
        for dup in dups {
            println!("{name}  {dup}");
            lines += 1;
            if dup.ends_with("[DUPLICATE]") {
                duplicates += 1;
            }
        }
    }
    println!("cases with an extra at a right position: {}", rows.len());
    println!("lines: {lines}  ({duplicates} DUPLICATE, {} shadow)", lines - duplicates);
}

/// The extras whose `(file, line, column)` the baseline also records — i.e. a
/// second diagnostic at a position that already carries the right one. The
/// printed form names both codes, since which of the two is upstream's is the
/// first question every such row raises.
fn measure(case: &CaseEntry) -> Option<(String, Vec<String>)> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return None;
    }
    let test = case.load().ok()?;
    let actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    let dups: Vec<String> = actual
        .iter()
        .filter(|d| !expected.contains(d))
        .filter_map(|extra| {
            let at = expected.iter().find(|want| {
                want.file == extra.file && want.line == extra.line && want.column == extra.column
            })?;
            // **Two very different rows wear the same shape.** If this port
            // also emits the wanted code at that position, a second rule spoke
            // where upstream's chain stops after one — §858's TS1042 shape, and
            // removing the extra converts the position. If it does **not**, the
            // extra is the *shadow of a missing diagnostic*: upstream suppresses
            // it precisely because the wanted one fired, so the fix is to report
            // one MORE, not one fewer (§862 on TS2364, §863 on TS2357).
            let both = actual.contains(at);
            Some(format!(
                "{}({},{}) TS{} extra, beside TS{} wanted [{}]",
                extra.file,
                extra.line,
                extra.column,
                extra.code,
                at.code,
                if both { "DUPLICATE" } else { "shadow" }
            ))
        })
        .collect();
    if dups.is_empty() { None } else { Some((case.name.clone(), dups)) }
}
