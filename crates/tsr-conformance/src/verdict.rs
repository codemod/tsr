//! The verdict-row computation shared by `examples/verdictdump.rs` and
//! `examples/scorepair.rs` — one walk, two consumers, so the scoring pair
//! and the raw dump can never drift.
//!
//! A row is `case:file:position<TAB>verdict<TAB>want<TAB>got`, keyed
//! positionally (case name, file index, assertion position) because a
//! `.types` baseline repeats a line like `>x : number` many times in one
//! file and a text key would collapse them. The population and alignment
//! shape are `wrongdelta`'s: same skips, same `has_varied_types` /
//! known-divergence filter, same `assertions_for_case_with_ids` walk.

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use crate::{Corpus, repo_root, types_baseline, types_producer};

/// Every aligned assertion line for cases whose NAME contains any of the
/// `filter` substrings — or every case when `filter` is empty. Sorted by
/// key so two runs diff line-for-line.
#[must_use]
pub fn verdict_rows(filter: &[String]) -> Vec<String> {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let mut rows: Vec<String> = cases
        .par_iter()
        .flat_map_iter(|case| {
            let mut out = Vec::new();
            if !filter.is_empty() && !filter.iter().any(|needle| case.name.contains(needle)) {
                return out;
            }
            if case.has_varied_types() || case.has_known_divergence() {
                return out;
            }
            let Some(text) = case.expected_types() else { return out };
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return out;
            }
            let Ok(parsed) = case.load() else { return out };
            let arena = tsr_core::Arena::new();
            let (_program, ours, _ids) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
            for (index, expected_file) in expected.iter().enumerate() {
                let Some(our_file) = ours.get(index) else { continue };
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = our_file.get(position) else { continue };
                    let got_line = got.line();
                    // RIGHT first: an exact match on the whole line.
                    if want.text == got_line {
                        out.push(format!(
                            "{}:{index}:{position}\tRIGHT\t{}\t{}",
                            case.name, want.text, got_line
                        ));
                        continue;
                    }
                    // Unaligned — the baseline and this port disagree about
                    // the *expression*, so no comparison of answers is
                    // meaningful and the line belongs to no bucket.
                    let (Some((we, wt)), Some((ge, gt))) = (
                        want.split(),
                        types_baseline::TypeAssertion { text: got_line.clone() }
                            .split()
                            .map(|(e, t)| (e.to_string(), t.to_string())),
                    ) else {
                        continue;
                    };
                    if we != ge.as_str() {
                        continue;
                    }
                    let verdict = if gt == "error" { "GAP" } else { "WRONG" };
                    out.push(format!("{}:{index}:{position}\t{verdict}\t{wt}\t{gt}", case.name));
                }
            }
            out
        })
        .collect();
    rows.sort();
    rows
}

/// The `TOTAL … right … gap … wrong` summary line the dump prints to
/// stderr, computed the same way for both consumers.
#[must_use]
pub fn summary(rows: &[String]) -> String {
    let count = |what: &str| rows.iter().filter(|r| r.contains(&format!("\t{what}\t"))).count();
    format!(
        "TOTAL {}  right {}  gap {}  wrong {}",
        rows.len(),
        count("RIGHT"),
        count("GAP"),
        count("WRONG")
    )
}
