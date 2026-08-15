//! Cases that fail on the assertion COUNT — the failures
//! `target/verdict_baseline.tsv` cannot see.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example countgap
//! ```
//!
//! # Why this is a blind spot and not a detail
//!
//! `verdict.rs` emits one row per ALIGNED line: a position where the baseline
//! and this port agree about the *expression* so their answers can be compared.
//! A case that emits the wrong NUMBER of assertions has positions that align
//! with nothing, and they appear in no row at all.
//!
//! §636 found this the expensive way. `castOfYield` carries a verdict deficit of
//! **1**; converting that one line did not convert the case, because it emits 8
//! assertions where the baseline has 9. Corpus-wide the gap is visible in the
//! totals — **3,535 cases fail while only 3,381 carry a non-right aligned
//! line** — so ~154 fail for reasons every instrument built this window is blind
//! to (`TSR_ANY_DUMP`, `gapdump`, `writetarget`, and the single-transition join
//! all key on aligned rows).
//!
//! This probe reports the population directly: per case, how many assertions
//! this port emits against how many the baseline expects, and the signed delta.
//! A NEGATIVE delta means the port emits too few — a node the walk does not
//! select, or a file it renders empty. A POSITIVE delta means it emits lines
//! upstream does not, which shifts every later position in the file and is why
//! §247 recorded that *making* a heritage `ExpressionWithTypeArguments` visited
//! cost 458 cases.

use rayon::prelude::*;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let rows: Vec<(String, usize, usize)> = cases
        .par_iter()
        .filter_map(|case| {
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
            let (_program, ours, _ids) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);

            let mut mismatched = Vec::new();
            for (index, expected_file) in expected.iter().enumerate() {
                let want = expected_file.assertions.len();
                let got = ours.get(index).map_or(0, Vec::len);
                if want != got {
                    mismatched.push((format!("{}#{index}", case.name), got, want));
                }
            }
            Some(mismatched)
        })
        .flatten()
        .collect();

    let mut deltas: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    for (_, got, want) in &rows {
        #[expect(clippy::cast_possible_wrap, reason = "assertion counts are small")]
        let delta = *got as i64 - *want as i64;
        *deltas.entry(delta).or_default() += 1;
    }
    let too_few = rows.iter().filter(|(_, g, w)| g < w).count();
    let too_many = rows.iter().filter(|(_, g, w)| g > w).count();

    println!("files whose assertion COUNT differs: {}", rows.len());
    println!("  port emits TOO FEW:  {too_few}");
    println!("  port emits TOO MANY: {too_many}");

    println!("\nmost common deltas (got - want):");
    let mut ranked: Vec<_> = deltas.into_iter().collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (delta, n) in ranked.iter().take(12) {
        println!("  {n:>4}  {delta:+}");
    }

    // §638: for a file short by exactly ONE, name the missing assertion. The
    // first position where the expression differs is where the walk skipped a
    // node, and its expected text identifies the node kind.
    if std::env::var("TSR_COUNT_MISSING").is_ok() {
        println!("\n-- files short by exactly 1: the first divergent expected line --");
        let mut misses: Vec<String> = cases
            .par_iter()
            .filter_map(|case| {
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
                let (_p, ours, _i) =
                    types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
                let mut out = Vec::new();
                for (index, expected_file) in expected.iter().enumerate() {
                    let got = ours.get(index).map_or(0, Vec::len);
                    if expected_file.assertions.len() != got + 1 {
                        continue;
                    }
                    let Some(our_file) = ours.get(index) else { continue };
                    for (position, want) in expected_file.assertions.iter().enumerate() {
                        let ours_here = our_file
                            .get(position)
                            .map(tsr_conformance::types_producer::Assertion::line);
                        let same = ours_here
                            .as_deref()
                            .zip(want.split())
                            .is_some_and(|(line, (we, _))| line.starts_with(&format!("{we} : ")));
                        if !same {
                            out.push(format!("{}\t{}", case.name, want.text));
                            break;
                        }
                    }
                }
                Some(out)
            })
            .flatten()
            .collect();
        misses.sort();
        for m in &misses {
            println!("  {m}");
        }
        println!("  ({} listed)", misses.len());
    }

    println!("\nlargest shortfalls:");
    let mut worst = rows.clone();
    // Largest shortfall first: `want - got`, saturating so the too-many side
    // sorts to zero rather than wrapping.
    worst.sort_by_key(|(_, got, want)| std::cmp::Reverse(want.saturating_sub(*got)));
    for (name, got, want) in worst.iter().take(12) {
        println!("  {name:<62} got {got:>4} want {want:>4}");
    }
}
