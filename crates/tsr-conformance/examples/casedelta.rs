//! Per-case `checker_types` tallies, so a commit pair can be read case by case.
//!
//! # Why
//!
//! The gradient is one number over 9,538 cases, and a commit that moves it by
//! +431 lines can be a hundred cases gaining four each or one case gaining
//! 1,700 while eighty lose. Those are different facts and they license different
//! next moves, and until now nothing here could tell them apart: the committed
//! snapshot carries totals and a skip breakdown, not per-case tallies.
//!
//! This came up on the `@lib` directive fix. `bd tsr-cug` sized it at **1,784
//! lines** from `compiler/temporal` alone; the corpus measured **+431 net**.
//! Both can be true only if something else lost lines, and the whole point of
//! honouring `@lib` is that it moves in *both* directions — a case saying
//! `@lib: es5` asks for **fewer** libs than the target default, so this port had
//! been resolving names upstream cannot and answering a confident type where
//! upstream answers `any`. Netting is exactly what hides that.
//!
//! # Use
//!
//! ```text
//! cargo run --release -p tsr-conformance --example casedelta > before.tsv
//! # ... change something ...
//! cargo run --release -p tsr-conformance --example casedelta > after.tsv
//! join -t $'\t' before.tsv after.tsv | awk -F'\t' '$2 != $4'
//! ```
//!
//! Output is `name<TAB>matched<TAB>total`, one judged case per line, sorted by
//! name so `join` and `diff` both work on it without a pre-sort. Skipped cases
//! are **omitted**, which is what keeps the file joinable across a change that
//! alters nothing about the skip set — and a change that *does* alter the skip
//! set shows up as an unmatched key rather than as a silently shifted row.
//!
//! # The numbers are the suite's, not a re-derivation
//!
//! `matched` and `total` come from `types_suite::compare`, and the skip
//! predicates are the suite's own in the suite's own order. That matters twice
//! over:
//!
//! - `compare` counts **upstream's baseline lines** (`assertion_count(expected)`),
//!   not the lines we rendered. A probe that iterates `our_file` instead is
//!   counting a different population — 479,060 against 478,954, reconciled in
//!   `examples/reconcile.rs` (`bd tsr-zlo`).
//! - Asking for the plain `.types` before testing `has_varied_types` would send
//!   all 2,032 configuration-varied cases into the "no baseline" bucket
//!   (`types_suite.rs:184`).
//!
//! So the column sums here are the gradient, exactly, and `TOTAL` is printed on
//! stderr as the control that says so.

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::types_baseline::{self, FileTypes};
use tsr_conformance::{Corpus, repo_root, types_producer, types_suite};

/// One judged case.
struct Row {
    name: String,
    matched: usize,
    total: usize,
}

fn measure(case: &tsr_conformance::CaseEntry) -> Option<Row> {
    // The suite's skips, skip for skip, in the suite's order.
    if case.has_varied_types() || case.has_known_divergence() {
        return None;
    }
    let text = case.expected_types()?;
    let expected = types_baseline::parse(&text);
    let total = types_baseline::assertion_count(&expected);
    if total == 0 {
        return None;
    }

    // A case that does not load is judged, with every baseline line counted and
    // none matched — not skipped. Dropping it here would make this file's column
    // sum disagree with the gradient.
    let Ok(parsed) = case.load() else {
        return Some(Row { name: case.name.clone(), matched: 0, total });
    };

    let ours: Vec<FileTypes> = types_producer::assertions_for_case(&parsed, &expected, false)
        .iter()
        .zip(&expected)
        .map(|(rendered, expected_file)| {
            types_producer::to_file_types(&expected_file.file, rendered)
        })
        .collect();

    let comparison = types_suite::compare(&expected, &ours);
    Some(Row { name: case.name.clone(), matched: comparison.lines.matched, total })
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let mut rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));

    let mut matched = 0usize;
    let mut total = 0usize;
    for row in &rows {
        matched += row.matched;
        total += row.total;
        println!("{}\t{}\t{}", row.name, row.matched, row.total);
    }

    // stderr, so the TSV on stdout stays joinable. This is the control: these
    // two numbers must equal the committed snapshot's, or the rows above are
    // over some other population than the gradient.
    eprintln!("TOTAL cases {} matched {matched} total {total}", rows.len());
}
