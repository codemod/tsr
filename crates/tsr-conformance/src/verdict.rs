//! The verdict-row computation shared by `examples/verdictdump.rs` and
//! `examples/scorepair.rs` — one walk, two consumers, so the scoring pair
//! and the raw dump can never drift.
//!
//! A row is `case:file:position<TAB>verdict<TAB>want<TAB>got`, keyed
//! positionally (case name, file index, assertion position) because a
//! `.types` baseline repeats a line like `>x : number` many times in one
//! file and a text key would collapse them. The population and alignment
//! shape are `wrongdelta`'s: same skips, same `has_varied_types` /
//! known-divergence filter, same `assertions_for_case_with_ids` walk — plus
//! one row set per named configuration of a varied case
//! ([`crate::Corpus::configured`]), keyed by its suffixed name.

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use crate::case_guard::{self, RowShape};
use crate::{Corpus, repo_root, types_baseline, types_producer};

/// Every aligned assertion line for cases whose NAME contains any of the
/// `filter` substrings — or every case when `filter` is empty. Sorted by
/// key so two runs diff line-for-line.
///
/// Each case runs under [`case_guard::run_case`]: a case that panics yields
/// one `case:*:*<TAB>PANIC` row instead of aborting the walk. The rows carry
/// no guard columns, because `scorepair` stores them as its baseline; see
/// [`verdict_rows_guarded`].
#[must_use]
pub fn verdict_rows(filter: &[String]) -> Vec<String> {
    rows(filter, false)
}

/// [`verdict_rows`] with the case's two guard columns, `ms=<wall>` and
/// `mib=<peak memory>`, appended to every row (`tsr-2zk.1041`). The columns
/// before them keep their bytes; `examples/verdictdump.rs` prints these.
#[must_use]
pub fn verdict_rows_guarded(filter: &[String]) -> Vec<String> {
    rows(filter, true)
}

fn rows(filter: &[String], guard_columns: bool) -> Vec<String> {
    let corpus = Corpus::from_repo_root(&repo_root());
    let mut cases = corpus.discover().expect("corpus");
    // Each named configuration of a varied case is its own population,
    // keyed `case(target=es2015):file:position`, so no existing key moves
    // (ADR-0047). The varied case as itself stays out, below.
    let configured = Corpus::configured(&cases);
    cases.extend(configured);
    let mut rows: Vec<String> = cases
        .par_iter()
        .flat_map_iter(|case| {
            if !filter.is_empty() && !filter.iter().any(|needle| case.name.contains(needle)) {
                return Vec::new();
            }
            let measured = case_guard::run_case(&case.name, || case_rows(case));
            let columns = measured.columns();
            match measured.value {
                Ok(rows) if guard_columns => {
                    rows.into_iter().map(|row| format!("{row}\t{columns}")).collect()
                }
                Ok(rows) => rows,
                Err(message) => {
                    vec![RowShape::Types.marker(&case.name, "PANIC", &message, &columns)]
                }
            }
        })
        .collect();
    rows.sort();
    rows
}

/// One case's aligned rows, unsorted and without guard columns.
fn case_rows(case: &crate::corpus::CaseEntry) -> Vec<String> {
    let mut out = Vec::new();
    if (case.configuration.is_none() && case.has_varied_types()) || case.has_known_divergence() {
        return out;
    }
    let Some(text) = case.expected_types() else { return out };
    if types_baseline::assertion_count(&types_baseline::parse(&text)) == 0 {
        return out;
    }
    let Ok(parsed) = case.load() else { return out };
    // The reading `types_suite` judges: echoed code lines are not assertions.
    let expected = types_producer::expected_for_case(&text, &parsed);
    if types_baseline::assertion_count(&expected) == 0 {
        return out;
    }
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
            // §883: `TSR_VERDICT_EXPR=1` appends the EXPRESSION as a
            // fifth column. `we` is computed here to decide alignment
            // and was then thrown away, and every investigation from
            // §872 onward wanted it back: a row saying
            // `position -> expression -> want -> got` is the
            // difference between "some accesses re-union `undefined`
            // and some do not" (§881, which could go no further) and
            // knowing WHICH.
            //
            // Env-gated because `scorepair` parses these rows by tab
            // and a fifth column would change the format it reads —
            // §827's treatment, for the same reason.
            if std::env::var_os("TSR_VERDICT_EXPR").is_some() {
                out.push(format!("{}:{index}:{position}\t{verdict}\t{wt}\t{gt}\t{we}", case.name));
            } else {
                out.push(format!("{}:{index}:{position}\t{verdict}\t{wt}\t{gt}", case.name));
            }
        }
    }
    out
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
