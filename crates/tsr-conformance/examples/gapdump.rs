//! Per-line `gap_reason` for every GAP line, keyed to join against the verdict
//! baseline.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example gapdump
//! ```
//!
//! # Why this exists beside `gaproot.rs`
//!
//! `gaproot.rs` ranks gap CAUSES corpus-wide, which answers *"what is the
//! biggest gap"*. That is the right question for the gradient and the wrong one
//! for CASES: a cause holding 4,000 lines spread over cases that each fail for
//! six other reasons converts nothing when it lands.
//!
//! The forecastable quantity is a cause's intersection with the
//! **single-transition population** — the cases whose only non-right line is
//! one line, so converting it flips the case. This writes the left side of that
//! join, exactly as `TSR_ANY_DUMP=1` does for the `any` population (STATUS §6),
//! and it is the same method that found §584 and §585.
//!
//! The key is `case:index:position`, byte-for-byte the one `verdict.rs:70`
//! writes, so:
//!
//! ```text
//! awk -F'\t' '$2=="GAP"' target/verdict_baseline.tsv   # ... restricted to single-transition cases
//! join -t$'\t' <keys> <(sort -k1,1 target/gap_lines.tsv)
//! ```
//!
//! # The control
//!
//! `GAP` is defined here exactly as `verdict.rs:69` defines it — the aligned
//! line whose printed type is the string `error` — rather than by asking the
//! checker for `intrinsics.error`. Those two are not the same set (the writer
//! prints `any` for some error-typed nodes, `type_symbol_baseline.go:380`), and
//! a probe that used the second would silently rank a different population than
//! the board it claims to forecast. The printed total is asserted against the
//! suite's own gap count by the caller.

use rayon::prelude::*;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let rows: Vec<String> = cases
        .par_iter()
        .filter_map(|case| {
            // The suite's own skips, so the denominator is the gradient's.
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
            let node_map = program.node_map();
            let bound = program.binder();
            let mut checker = tsr_checker::Checker::new(bound, nodes, node_map);

            let mut out = Vec::new();
            for (index, expected_file) in expected.iter().enumerate() {
                let Some(our_file) = ours.get(index) else { continue };
                let our_ids = ids.get(index);
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = our_file.get(position) else { continue };
                    let got_line = got.line();
                    if want.text == got_line {
                        continue;
                    }
                    // `verdict.rs:60-68`'s alignment test, reproduced rather
                    // than approximated: an unaligned line belongs to no bucket.
                    let (Some((we, wt)), Some((ge, gt))) = (
                        want.split(),
                        types_baseline::TypeAssertion { text: got_line.clone() }
                            .split()
                            .map(|(e, t)| (e.to_string(), t.to_string())),
                    ) else {
                        continue;
                    };
                    if we != ge.as_str() || gt != "error" {
                        continue;
                    }
                    let Some(id) = our_ids.and_then(|line_ids| line_ids.get(position).copied())
                    else {
                        continue;
                    };
                    let reason =
                        types_producer::gap_reason(&mut checker, bound, nodes, node_map, id);
                    out.push(format!("{}:{index}:{position}\t{reason}\t{wt}", case.name));
                }
            }
            Some(out)
        })
        .flatten()
        .collect();

    let mut rows = rows;
    rows.sort_unstable();
    let path = root.join("target/gap_lines.tsv");
    std::fs::write(&path, rows.join("\n")).expect("write gap dump");
    println!("gapdump: wrote {} GAP lines to {}", rows.len(), path.display());
}
