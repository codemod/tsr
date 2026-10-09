//! Where a case's `.types` lines stop aligning: the first position, per
//! baseline section, at which this port's walker and upstream's disagree about
//! the **expression** (`docs/parity/notes/r5-align.md`).
//!
//! ```text
//! TSR_FILTER=compiler/castOfYield cargo run --release -p tsr-conformance --example unaligned
//! TSR_FILTER=... TSR_CONTEXT=3 cargo run ...   # lines around the divergence
//! ```
//!
//! A line aligns when upstream's line starts with `{our expression} : ` (the
//! prefix test `types_producer::walker_agreement` uses: the line cannot be
//! split, because the expression may contain `" : "`). An unaligned line is in
//! neither dump, so `verdictdump` cannot show it; this prints, per section,
//! the counts on both sides and the first divergence with `TSR_CONTEXT` lines
//! of each side after it.

use std::fmt::Write as _;

use rayon::prelude::*;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

fn main() {
    let filter: Vec<String> = std::env::var("TSR_FILTER")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let context: usize =
        std::env::var("TSR_CONTEXT").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
    let corpus = Corpus::from_repo_root(&repo_root());
    let mut cases = corpus.discover().expect("corpus");
    let configured = Corpus::configured(&cases);
    cases.extend(configured);
    let mut reports: Vec<(String, String)> = cases
        .par_iter()
        .filter(|case| filter.is_empty() || filter.contains(&case.name))
        .filter_map(|case| {
            if (case.configuration.is_none() && case.has_varied_types())
                || case.has_known_divergence()
            {
                return None;
            }
            let text = case.expected_types()?;
            let parsed = case.load().ok()?;
            let expected = types_producer::expected_for_case(&text, &parsed);
            if types_baseline::assertion_count(&expected) == 0 {
                return None;
            }
            let arena = tsr_core::Arena::new();
            let (program, ours, ids) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
            let mut out = String::new();
            for (index, want) in expected.iter().enumerate() {
                let got = ours.get(index).map(Vec::as_slice).unwrap_or_default();
                let first = want.assertions.iter().enumerate().position(|(at, line)| {
                    got.get(at).is_none_or(|g| !line.text.starts_with(&format!("{} : ", g.text)))
                });
                let first =
                    first.or((got.len() > want.assertions.len()).then_some(want.assertions.len()));
                let Some(first) = first else { continue };
                let _ = writeln!(
                    out,
                    "  [{index}] {}: want {} got {}, first divergence at {first}",
                    want.file,
                    want.assertions.len(),
                    got.len()
                );
                for at in first..first + context.max(1) {
                    let w = want.assertions.get(at).map_or("-", |a| a.text.as_str());
                    let parent = ids
                        .get(index)
                        .and_then(|ids| ids.get(at))
                        .and_then(|&id| program.nodes().parent(id))
                        .map(|p| program.nodes().kind(p));
                    let g = got.get(at).map_or_else(
                        || "-".to_string(),
                        |a| format!("{:?} (in {parent:?}) {}", a.kind, a.line()),
                    );
                    let _ = writeln!(out, "    {at:>4} want >{w}\n         got  >{g}");
                }
            }
            (!out.is_empty()).then(|| (case.name.clone(), out))
        })
        .collect();
    reports.sort();
    for (name, report) in reports {
        println!("{name}\n{report}");
    }
}
