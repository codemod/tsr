//! Every aligned **wrong** line as `case<TAB>want<TAB>got` — `casedelta`'s
//! sibling for the wrong bucket.
//!
//! `casedelta` is blind to gap→wrong by construction: a gap turning wrong
//! moves neither `matched` nor a case verdict, and both sessions that built
//! member instantiation (`bd tsr-4qx`, `bd tsr-0hc`) needed exactly that
//! attribution. Two runs of this probe — before via `git stash`, after — and a
//! `comm -13` name each new wrong line with the pair of strings that disagree,
//! which is what turned "Δwrong = +538" into "490 lines are `then` rendered
//! under the wrong `strictNullChecks`" in one command.
//!
//! The population is `wrongflip.rs`'s (same skips, same alignment shape), but
//! the output is the raw joinable dump rather than a classification: this
//! probe answers *what changed*, `wrongflip` answers *why the standing bucket
//! is wrong*. The `TOTAL` on stderr is comparable across commits only from
//! the same probe — `STATUS.md` §1 records a cross-instrument subtraction
//! that misread a bar by 4,000 lines.

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
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
            for (index, expected_file) in expected.iter().enumerate() {
                let Some(our_file) = ours.get(index) else { continue };
                for (i, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = our_file.get(i) else { continue };
                    let got_line = got.line();
                    if want.text == got_line {
                        continue;
                    }
                    // Same expression, different type: the checker's wrong
                    // line, not the walker's. A gap (`: error`) is excluded.
                    let (Some((we, wt)), Some((ge, gt))) = (
                        want.split(),
                        types_baseline::TypeAssertion { text: got_line.clone() }
                            .split()
                            .map(|(e, t)| (e.to_string(), t.to_string())),
                    ) else {
                        continue;
                    };
                    let (ge, gt) = (ge.as_str(), gt.as_str());
                    if we == ge && wt != gt && gt != "error" {
                        out.push(format!("{}\t{}\t{}", case.name, wt, gt));
                    }
                }
            }
            out
        })
        .collect();
    rows.sort();
    for row in &rows {
        println!("{row}");
    }
    eprintln!("TOTAL wrong lines {}", rows.len());
}
