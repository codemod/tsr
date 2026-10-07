//! Run ONE ad-hoc source file through the REAL corpus pipeline — the
//! bundled libs, the `@strict`/directive parsing, the `@filename` splits —
//! and print every assertion the walker produces.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example probefile -- /tmp/repro.ts
//! ```
//!
//! # Why this exists
//!
//! Hypothesis probes kept hand-rolling parse→bind→check on a scratch file,
//! which "silently loses the `/.lib` mount and the `@libFiles` roots"
//! (`types_producer::program_for_case`'s own warning) — the identical
//! defect `writer_guards.rs` and `overload_funnel.rs` were rewired for
//! (`731b1ee`, `checker-notes-guard.md`), committed a third time as
//! throwaway probes during the §52 investigation: the probe answered
//! `number` while the corpus disagreed, because the probe was not running
//! in the corpus's world. A micro-repro proves nothing unless it runs the
//! same pipeline the gradient measures.

use tsr_conformance::{types_baseline::FileTypes, types_producer};

fn main() {
    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("PROBE_FILE").ok())
        .expect("usage: probefile <path-to-ts-file>   (or PROBE_FILE=…)");
    let source = std::fs::read_to_string(&path).expect("read probe file");
    let basename = std::path::Path::new(&path)
        .file_name()
        .map_or_else(|| "probe.ts".to_string(), |n| n.to_string_lossy().into_owned());
    let case = tsr_conformance::TestCase::parse("probe/adhoc", &basename, &source);
    // An empty expected-assertions list per unit: `render_case` renders every
    // assertion in each listed file; the expected list only drives alignment.
    let expected: Vec<FileTypes> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let rendered = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    for (unit, assertions) in case.files.iter().zip(&rendered) {
        println!("=== {} ===", unit.name);
        for assertion in assertions {
            println!(">{}", assertion.line());
        }
    }
}
