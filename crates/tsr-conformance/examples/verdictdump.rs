//! Every aligned assertion line as `case:file:position<TAB>verdict<TAB>want<TAB>got`
//! — the probe that makes a **transition** measurable rather than inferred.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example verdictdump > before.tsv
//! TSR_FILTER=caseA,caseB cargo run --release -p tsr-conformance --example verdictdump
//! ```
//!
//! Every row ends with two guard columns, `ms=<wall>` and `mib=<peak memory>`
//! for the row's case (`tsr_conformance::case_guard`, `tsr-2zk.1041`), after
//! the four above, which keep their bytes (with `TSR_VERDICT_EXPR=1`, after
//! the fifth). A case that panics prints one `case:*:*<TAB>PANIC` row; one the
//! watchdog stops prints `OOM` or `TIMEOUT` and ends the run with exit status
//! 3. `examples/slowcases.rs` reads the guard columns;
//! `docs/parity/notes/r5-harness.md` says how the gate uses them.
//!
//! `TSR_FILTER` (comma-separated case-name substrings) restricts the run for
//! the fast inner loop; the scoring pair before a landing must run UNFILTERED
//! — see `examples/scorepair.rs`, which owns the diff protocol.
//!
//! # Why this exists beside `casedelta` and `wrongdelta`
//!
//! `casedelta` reports per-case tallies, so it cannot say which *line* moved.
//! `wrongdelta` dumps the wrong bucket, so it can say a line **arrived** in it
//! but never where the line came from: a gap→wrong arrival and a right→wrong
//! arrival are the same row in its output. `checker-notes-qualname.md` §9.6 and
//! the `docs/conventions.md` entry it bought turn on exactly that distinction.
//!
//! The row computation lives in `tsr_conformance::verdict` so this dump and
//! `scorepair` can never drift.

use tsr_conformance::case_guard::{self, RowShape};

#[path = "support/counting_alloc.rs"]
mod counting_alloc;

fn main() {
    case_guard::install(RowShape::Types);
    let filter: Vec<String> = std::env::var("TSR_FILTER")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let rows = tsr_conformance::verdict::verdict_rows_guarded(&filter);
    for row in &rows {
        println!("{row}");
    }
    eprintln!("{}", tsr_conformance::verdict::summary(&rows));
}
