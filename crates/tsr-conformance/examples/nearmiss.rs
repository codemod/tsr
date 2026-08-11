//! The **case** board: every `checker_types` case blocked by a small number of
//! non-right lines, with those lines dumped as `want` beside `got`.
//!
//! # Why this is a different board from every other one here
//!
//! `checker_types` reports two numbers and they reward different work. The line
//! **gradient** is `matched / total` over all 478,954 baseline lines; the
//! **case pass rate** counts a case only when *every* line in it is right. §173
//! measured the divergence: the same window's largest gradient arm (§167, +328
//! lines) moved 30 cases — 0.09 cases per line — while a 27-line arm moved 16
//! cases, six times the leverage per line. Ranking by lines is therefore close
//! to *anti*-optimal for the case rate, and no instrument in `examples/` ranked
//! by the case rate: `casedelta` prints the per-case tally but not what the
//! failing lines *are*, and `wrongdelta`/`gaproot` rank line populations over
//! the whole corpus, where a case one line from passing is indistinguishable
//! from a case a thousand lines from passing.
//!
//! This probe answers the case-rate question directly: **which cases are nearly
//! passing, and what exactly is in the way.** A family that appears across many
//! near-miss cases is worth more than its line count says, because each line it
//! converts may flip a whole case.
//!
//! # Use
//!
//! ```text
//! cargo run --release -p tsr-conformance --example nearmiss -- --summary
//!     the pool by deficit, with the cumulative case rate each band is worth
//!
//! cargo run --release -p tsr-conformance --example nearmiss -- --max 2 > pool.tsv
//!     every blocking line in every case with deficit <= 2, joinable TSV
//!
//! cargo run --release -p tsr-conformance --example nearmiss -- --max 1 --shapes
//!     the same pool ranked by what the blocked lines WANT
//!
//! cargo run --release -p tsr-conformance --example nearmiss -- --structural
//!     the deficit-0 pool: every baseline line right, the case still failing
//!
//! cargo run --release -p tsr-conformance --example nearmiss -- --counts
//!     how much of the board is a WALK disagreement rather than a checker one
//!
//! cargo run --release -p tsr-conformance --example nearmiss -- --case <name>
//!     one case, every rendered line beside every baseline line, aligned
//! ```
//!
//! Row format is `case<TAB>deficit<TAB>file<TAB>position<TAB>want<TAB>got`, with
//! `got` the literal string `<absent>` when this port rendered no line at that
//! position at all and `want` `<none>` when it rendered one the baseline does
//! not have. Sorted by name, so two runs `diff` cleanly.
//!
//! # The population is the suite's
//!
//! Skips, alignment, the `matched` count and the **verdict** are
//! `types_suite::compare`'s own, so `--summary`'s `passing` must equal the
//! committed snapshot's passing-case count exactly. That equality is the
//! control: print it, check it, and if it disagrees this probe is over some
//! other population than the suite and nothing it ranks is readable.
//! `casedelta`'s header explains why `has_varied_types` must be tested before
//! `expected_types` is asked for.
//!
//! The control **fired on the first run**, and its answer is a pool: reading
//! `passing` as `deficit == 0` gave 4,419 against the snapshot's 4,392.
//! `compare` fails a case for a section count or an assertion count that
//! differs from the baseline's, and **27 cases match every baseline line while
//! rendering a different number of them** (`--structural`). Those are the
//! cheapest cases in the corpus — zero wrong types — and no other board here
//! can see them, because every other board ranks lines and these have no bad
//! line to rank.

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::collections::{BTreeMap, BTreeSet};
use tsr_conformance::types_baseline::{self, FileTypes};
use tsr_conformance::{Corpus, repo_root, types_producer, types_suite};

/// One blocked line: the baseline's text and ours at the same position.
struct Blocked {
    file: String,
    position: usize,
    want: String,
    got: Option<String>,
}

/// One judged case, with its blocked lines when there are few enough to name.
struct Row {
    name: String,
    deficit: usize,
    /// The suite's own verdict. **Not** `deficit == 0`: `compare` also fails a
    /// case whose section count or assertion count differs from the baseline's,
    /// and 27 cases at `5b1047d` match every baseline line while rendering a
    /// different NUMBER of them. Counting those as passing would put this
    /// probe's rate 27 above the snapshot's, and a board that disagrees with
    /// the gate it is ranking work for is not readable.
    passed: bool,
    /// `compare`'s own first reason, kept so the structural pool can be read
    /// without a second run: for a `deficit == 0` failure the reason IS the
    /// whole defect.
    reason: Option<String>,
    /// `sum(ours) - sum(baseline)` over the case's file sections. Non-zero means
    /// the two walks disagree about *how many* expressions the file has, which
    /// misaligns every line after the first divergence — so this is the one
    /// number that says whether a case's line tally is even meaningful.
    count_delta: isize,
    blocked: Vec<Blocked>,
}

/// Beyond this, listing the lines is noise — the case is not near anything. The
/// tally still counts it, so `--summary`'s denominator stays the whole suite.
const LIST_LIMIT: usize = 8;

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

    // A case that does not load is judged with nothing matched, exactly as
    // `compare` would judge it — not skipped.
    let Ok(parsed) = case.load() else {
        return Some(Row {
            name: case.name.clone(),
            deficit: total,
            passed: false,
            reason: Some("case did not load".into()),
            count_delta: 0,
            blocked: Vec::new(),
        });
    };

    let ours: Vec<FileTypes> = types_producer::assertions_for_case(&parsed, &expected, false)
        .iter()
        .zip(&expected)
        .map(|(rendered, expected_file)| {
            types_producer::to_file_types(&expected_file.file, rendered)
        })
        .collect();

    let comparison = types_suite::compare(&expected, &ours);
    let deficit = total - comparison.lines.matched;
    let is_pass = comparison.mismatch.is_none();
    let reason = comparison.mismatch.clone();
    #[allow(clippy::cast_possible_wrap)]
    let count_delta = expected
        .iter()
        .enumerate()
        .map(|(index, file)| {
            ours.get(index).map_or(0, |f| f.assertions.len() as isize)
                - file.assertions.len() as isize
        })
        .sum();

    let mut blocked = Vec::new();
    if !is_pass && deficit <= LIST_LIMIT {
        // The same positional alignment `compare` uses, but run to the longer of
        // the two sides: a line this port invented past the baseline's end is
        // invisible to an iteration over the baseline, and that line is the
        // entire defect in the `deficit == 0` pool.
        let empty = Vec::new();
        for (index, expected_file) in expected.iter().enumerate() {
            let our_lines = ours.get(index).map_or(&empty, |f| &f.assertions);
            for position in 0..expected_file.assertions.len().max(our_lines.len()) {
                let want = expected_file.assertions.get(position).map(|a| a.text.clone());
                let got = our_lines.get(position).map(|a| a.text.clone());
                if want == got {
                    continue;
                }
                blocked.push(Blocked {
                    file: expected_file.file.clone(),
                    position,
                    want: want.unwrap_or_else(|| "<none>".into()),
                    got,
                });
            }
        }
    }

    Some(Row { name: case.name.clone(), deficit, passed: is_pass, reason, count_delta, blocked })
}

/// The type side of `>expr : type`, which is the part a checker arm decides.
/// Falls back to the whole line when the baseline row is not of that shape.
fn type_of(line: &str) -> String {
    types_baseline::TypeAssertion { text: line.to_string() }
        .split()
        .map_or_else(|| line.to_string(), |(_expr, ty)| ty.to_string())
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap, clippy::similar_names)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let summary = args.iter().any(|a| a == "--summary");
    let structural_only = args.iter().any(|a| a == "--structural");
    let shapes = args.iter().any(|a| a == "--shapes");
    let max = args
        .iter()
        .position(|a| a == "--max")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2);

    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let mut rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));

    if summary {
        // The cumulative column is the forecast: converting the whole pool at
        // deficit <= d is worth that many cases.
        let mut by_deficit: BTreeMap<usize, usize> = BTreeMap::new();
        let mut structural = 0usize;
        for row in &rows {
            if row.passed {
                continue;
            }
            if row.deficit == 0 {
                structural += 1;
            } else {
                *by_deficit.entry(row.deficit.min(LIST_LIMIT + 1)).or_default() += 1;
            }
        }
        let judged = rows.len();
        let passing = rows.iter().filter(|r| r.passed).count();
        println!(
            "judged {judged}  passing {passing}  rate {:.2}%",
            100.0 * passing as f64 / judged as f64
        );
        println!("deficit\tcases\tcumulative-if-converted\trate-then");
        let mut cumulative = passing + structural;
        println!(
            "0 (count/section only)\t{structural}\t{cumulative}\t{:.2}%",
            100.0 * cumulative as f64 / judged as f64
        );
        for (deficit, count) in &by_deficit {
            cumulative += count;
            let label =
                if *deficit > LIST_LIMIT { format!(">{LIST_LIMIT}") } else { deficit.to_string() };
            println!(
                "{label}\t{count}\t{cumulative}\t{:.2}%",
                100.0 * cumulative as f64 / judged as f64
            );
        }
        return;
    }

    if let Some(wanted) = args.iter().position(|a| a == "--case").and_then(|i| args.get(i + 1)) {
        // The structural pool needs a view no other instrument has: every line
        // this port rendered beside every line the baseline records, aligned by
        // position, so an EXTRA line is visible. `traceone` iterates the
        // baseline's positions and is blind to a line we invented past its end.
        let case = cases
            .iter()
            .find(|c| &c.name == wanted || c.name.ends_with(wanted.as_str()))
            .expect("case not found");
        let text = case.expected_types().expect("no .types baseline");
        let expected = types_baseline::parse(&text);
        let parsed = case.load().expect("case did not load");
        let ours: Vec<FileTypes> = types_producer::assertions_for_case(&parsed, &expected, false)
            .iter()
            .zip(&expected)
            .map(|(rendered, expected_file)| {
                types_producer::to_file_types(&expected_file.file, rendered)
            })
            .collect();
        for (index, expected_file) in expected.iter().enumerate() {
            println!("=== {} ===", expected_file.file);
            let empty = Vec::new();
            let our_lines = ours.get(index).map_or(&empty, |f| &f.assertions);
            for position in 0..expected_file.assertions.len().max(our_lines.len()) {
                let want = expected_file.assertions.get(position).map(|a| a.text.as_str());
                let got = our_lines.get(position).map(|a| a.text.as_str());
                let mark = if want == got { " " } else { "*" };
                println!(
                    "{mark}{position}\tWANT {}\tGOT {}",
                    want.unwrap_or("<none>"),
                    got.unwrap_or("<none>")
                );
            }
        }
        return;
    }

    if args.iter().any(|a| a == "--counts") {
        // How much of the board is a WALK disagreement rather than a checker
        // one. A case whose counts differ has its lines misaligned from the
        // first divergence on, so its deficit is not a count of wrong types and
        // no type-shaped board can read it.
        let mut over = 0usize;
        let mut under = 0usize;
        let mut over_lines = 0isize;
        for row in &rows {
            match row.count_delta.cmp(&0) {
                std::cmp::Ordering::Greater => {
                    over += 1;
                    over_lines += row.count_delta;
                }
                std::cmp::Ordering::Less => under += 1,
                std::cmp::Ordering::Equal => {}
            }
        }
        println!("cases rendering MORE assertions than the baseline: {over} (+{over_lines} lines)");
        println!("cases rendering FEWER:                             {under}");
        println!(
            "deficit carried by those cases: {}",
            rows.iter().filter(|r| r.count_delta != 0).map(|r| r.deficit).sum::<usize>()
        );
        return;
    }

    if structural_only {
        // Every baseline line right, the case still failed: the port rendered a
        // different number of assertions, or a different set of file sections.
        for row in rows.iter().filter(|r| !r.passed && r.deficit == 0) {
            println!("{}\t{}", row.name, row.reason.as_deref().unwrap_or("(none)"));
            for b in &row.blocked {
                println!(
                    "\t{}:{}\tWANT {}\tGOT {}",
                    b.file,
                    b.position,
                    b.want,
                    b.got.as_deref().unwrap_or("<none>")
                );
            }
        }
        return;
    }

    let pool = rows.iter().filter(|r| !r.passed && r.deficit > 0 && r.deficit <= max);

    if shapes {
        // What the blocked lines WANT, ranked. `cases` is the number of distinct
        // cases a shape blocks, which is the column that predicts conversions —
        // `lines` alone over-counts a shape that appears twice in one case.
        let mut want_lines: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for row in pool {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for b in &row.blocked {
                let key = type_of(&b.want);
                want_lines.entry(key.clone()).or_default().0 += 1;
                if seen.insert(key.clone()) {
                    want_lines.entry(key).or_default().1 += 1;
                }
            }
        }
        let mut ranked: Vec<_> = want_lines.into_iter().collect();
        ranked.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(&b.0)));
        println!("cases\tlines\twant-type");
        for (want, (lines, cases)) in ranked.iter().take(80) {
            println!("{cases}\t{lines}\t{want}");
        }
        return;
    }

    let mut listed = 0usize;
    for row in pool {
        listed += 1;
        for b in &row.blocked {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                row.name,
                row.deficit,
                b.file,
                b.position,
                b.want,
                b.got.as_deref().unwrap_or("<absent>")
            );
        }
    }
    eprintln!("cases with deficit in 1..={max}: {listed}");
}
