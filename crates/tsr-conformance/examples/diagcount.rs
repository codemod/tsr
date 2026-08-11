//! **Which cases emit a right line too many times?**
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagcount
//! ```
//!
//! The suite compares **sorted multisets** of `(file, line, column, code)`, so a
//! diagnostic emitted twice where the baseline wants it once fails the case
//! exactly as a wrong one does (§919). Every other instrument in this workstream
//! compares *sets*: `diagmissing` asks whether each expected line is present and
//! whether anything reported is unexpected, and **a second copy of an expected
//! line passes both tests**. `diagsole` inherits the blindness and so does the
//! ranking built on it.
//!
//! §992 paid for that: three fixtures were ranked "blocked on TS1235 alone" while
//! they were blocked on TS1235 *and* a doubled TS1344, and the only signal was a
//! build that measured `+0` with every right line present.
//!
//! This is the multiset view. A case appears when some `(file, line, column,
//! code)` is reported more often than the baseline records it, and the count is
//! the surplus.
//!
//! `checker-notes-diag2.md` §993.
use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

type Key = (String, u32, u32, u32);

/// `(case name, surplus lines, whether the surplus is the case's only defect)`.
type Row = (String, Vec<(Key, usize)>, bool);

fn key(d: &BaselineDiagnostic) -> Key {
    (d.file.clone(), d.line, d.column, d.code)
}

/// `(case, surplus lines, whether the surplus is the case's ONLY defect)`
fn measure(case: &CaseEntry) -> Option<Row> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    // A case with no expected diagnostics can have no *surplus of a wanted
    // line* by construction, and skipping it matches the other instruments —
    // which also avoids the deepest fixtures they have always avoided.
    if expected.is_empty() {
        return None;
    }
    let test = case.load().ok()?;
    let actual = tsr_conformance::diagnostics_suite::reported_for(&test);

    let mut wanted: HashMap<Key, usize> = HashMap::new();
    for d in &expected {
        *wanted.entry(key(d)).or_default() += 1;
    }
    let mut got: HashMap<Key, usize> = HashMap::new();
    for d in &actual {
        *got.entry(key(d)).or_default() += 1;
    }

    let mut surplus: Vec<(Key, usize)> = got
        .iter()
        .filter_map(|(k, &n)| {
            let want = wanted.get(k).copied().unwrap_or(0);
            (want > 0 && n > want).then(|| (k.clone(), n - want))
        })
        .collect();
    if surplus.is_empty() {
        return None;
    }
    surplus.sort_unstable();
    // **Sole defect**: every wanted line is present at least once, and nothing
    // is reported that the baseline does not record at all. Such a case
    // converts the moment the over-count stops.
    let only = wanted.iter().all(|(k, &n)| got.get(k).copied().unwrap_or(0) >= n)
        && got.keys().all(|k| wanted.contains_key(k));
    Some((case.name.clone(), surplus, only))
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();

    let mut by_code: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let (mut cases_only, mut lines_total) = (0usize, 0usize);
    for (name, surplus, only) in &rows {
        if *only {
            cases_only += 1;
            for ((file, line, column, code), n) in surplus {
                println!("[sole] {name}  {file}({line},{column}) TS{code} x{}", n + 1);
            }
        }
        for ((_, _, _, code), n) in surplus {
            let entry = by_code.entry(*code).or_default();
            entry.0 += n;
            if *only {
                entry.1 += 1;
            }
        }
        lines_total += surplus.iter().map(|(_, n)| n).sum::<usize>();
    }
    let mut ranked: Vec<(u32, usize, usize)> =
        by_code.into_iter().map(|(code, (lines, sole))| (code, lines, sole)).collect();
    ranked.sort_unstable_by_key(|&(code, lines, _)| (std::cmp::Reverse(lines), code));
    println!("\n{:>8}  {:>7}  {:>10}", "code", "surplus", "sole-cases");
    for (code, lines, sole) in ranked.iter().take(25) {
        println!("  TS{code:<6}{lines:>7}  {sole:>10}");
    }
    println!(
        "\n{} cases over-count a right line; {lines_total} surplus lines; \
         {cases_only} cases blocked by the over-count ALONE",
        rows.len()
    );
}
