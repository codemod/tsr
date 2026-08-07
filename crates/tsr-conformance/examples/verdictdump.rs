//! Every aligned assertion line as `case:file:position<TAB>verdict<TAB>want<TAB>got`
//! — the probe that makes a **transition** measurable rather than inferred.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example verdictdump > before.tsv
//! ```
//!
//! # Why this exists beside `casedelta` and `wrongdelta`
//!
//! `casedelta` reports per-case tallies, so it cannot say which *line* moved.
//! `wrongdelta` dumps the wrong bucket, so it can say a line **arrived** in it
//! but never where the line came from: a gap→wrong arrival and a right→wrong
//! arrival are the same row in its output. `checker-notes-qualname.md` §9.6 and
//! the `docs/conventions.md` entry it bought turn on exactly that distinction —
//!
//! > *"A gap→wrong line arriving through a mechanism's success is
//! > indistinguishable, in a `wrongdelta` total, from a line the mechanism got
//! > wrong."*
//!
//! — and design P's registered bar (§10.5) splits its fourth leg on it:
//! **`entering the wrong set FROM GAP == 0`** and
//! **`entering the wrong set FROM RIGHT ≤ 45`**. Those are two different
//! findings with two different diagnoses, and no existing instrument here
//! separates them.
//!
//! The key is positional — case, file index, assertion position — rather than
//! the assertion text, because a `.types` baseline repeats a line like
//! `>x : number` many times in one file and a text key would collapse them.
//!
//! The population and the alignment shape are `wrongdelta`'s, copied so the two
//! agree line for line: same skips, same `has_varied_types` / known-divergence
//! filter, same `assertions_for_case_with_ids` walk.
//!
//! Join two runs with `sort` and `join -t$'\t'` on field 1, or read the
//! transition matrix straight out of `awk`.

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let mut rows: Vec<String> = cases
        .par_iter()
        .flat_map_iter(|case| {
            let mut out = Vec::new();
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
                    // Unaligned — the baseline and this port disagree about the
                    // *expression*, so no comparison of answers is meaningful
                    // and the line belongs to no bucket. `wrongdelta` drops
                    // these the same way.
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
    for row in &rows {
        println!("{row}");
    }
    let count = |what: &str| rows.iter().filter(|r| r.contains(&format!("\t{what}\t"))).count();
    eprintln!(
        "TOTAL {}  right {}  gap {}  wrong {}",
        rows.len(),
        count("RIGHT"),
        count("GAP"),
        count("WRONG")
    );
}
