//! The scoring pair as ONE command: run the corpus, diff against the last
//! ACCEPTED baseline, print the transition matrix with per-case attribution.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example scorepair              # score vs baseline
//! TSR_FILTER=caseA cargo run --release -p tsr-conformance --example scorepair   # subset score
//! cargo run --release -p tsr-conformance --example scorepair -- --accept  # land: full run becomes the baseline
//! ```
//!
//! # The protocol this encodes
//!
//! Every build scores by the per-line TRANSITION MATRIX over the whole
//! corpus (`docs/conventions.md`, the §12.2 trace discipline): adverse
//! movement routinely lands in cases a change never touched, so a filtered
//! run may ITERATE but never LAND. Rules enforced here rather than
//! remembered:
//!
//! - `--accept` refuses to run filtered — a baseline must be a full run.
//! - A filtered score diffs only the filtered subset against the baseline's
//!   same subset, and SAYS it is partial in the output.
//! - The baseline lives at `target/verdict_baseline.tsv` — per-checkout
//!   state, never committed (it is 470k rows of derived data).
//!
//! Two stale-baseline mistakes in one session (`checker-notes-callres.md`
//! §36's mis-diffed first measurement) are why this is a tool and not a
//! shell habit.

use std::collections::BTreeMap;
use std::fmt::Write as _;

fn verdict_of(row: &str) -> (&str, &str) {
    let mut parts = row.splitn(4, '\t');
    let key = parts.next().unwrap_or("");
    let verdict = parts.next().unwrap_or("");
    (key, verdict)
}

/// The matrix's name for a line with no row on one side: no aligned
/// expression there, so no verdict.
const UNALIGNED: &str = "UNALIGNED";

fn case_of(key: &str) -> &str {
    key.rsplitn(3, ':').nth(2).unwrap_or(key)
}

fn main() {
    tsr_conformance::case_guard::size_worker_pool();
    let accept = std::env::args().any(|a| a == "--accept");
    let filter: Vec<String> = std::env::var("TSR_FILTER")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if accept && !filter.is_empty() {
        eprintln!("scorepair: --accept refuses a filtered run — a baseline must be a full run");
        std::process::exit(2);
    }
    let baseline_path = tsr_conformance::repo_root().join("target/verdict_baseline.tsv");

    let rows = tsr_conformance::verdict::verdict_rows(&filter);
    let summary = tsr_conformance::verdict::summary(&rows);

    let Ok(baseline_text) = std::fs::read_to_string(&baseline_path) else {
        if filter.is_empty() {
            std::fs::write(&baseline_path, rows.join("\n")).expect("write baseline");
            println!("{summary}");
            println!(
                "no baseline existed — this full run is now the baseline ({})",
                baseline_path.display()
            );
        } else {
            println!("{summary}");
            println!(
                "no baseline exists and this run is filtered — run unfiltered once to seed it"
            );
        }
        return;
    };

    let baseline: BTreeMap<&str, &str> = baseline_text
        .lines()
        .map(verdict_of)
        .filter(|(k, _)| {
            filter.is_empty() || {
                let case = case_of(k);
                filter.iter().any(|needle| case.contains(needle))
            }
        })
        .collect();
    let current: BTreeMap<&str, &str> = rows.iter().map(|r| verdict_of(r)).collect();

    // A row exists only for an ALIGNED line (`verdict::case_rows`), so a key
    // on one side alone is a line that changed alignment. It used to be
    // skipped, and a RIGHT line whose expression stopped aligning left the
    // matrix silently (`docs/parity/notes/r5-harness.md` §5 item 8). Both
    // directions are transitions to or from `UNALIGNED`; losing a RIGHT one
    // is adverse like any other RIGHT->non-RIGHT.
    let mut matrix: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (key, was) in &baseline {
        let now = current.get(key).copied().unwrap_or(UNALIGNED);
        if *was != now {
            matrix
                .entry(((*was).to_string(), now.to_string()))
                .or_default()
                .push((*key).to_string());
        }
    }
    for (key, now) in &current {
        if !baseline.contains_key(key) {
            matrix
                .entry((UNALIGNED.to_string(), (*now).to_string()))
                .or_default()
                .push((*key).to_string());
        }
    }

    println!("{summary}{}", if filter.is_empty() { "" } else { "   [PARTIAL: filtered run]" });
    // A case that panicked is one `case:*:*` row (`case_guard`), which keys
    // match nothing in the baseline: name it rather than let it vanish.
    for row in rows.iter().filter(|row| verdict_of(row).1 == "PANIC") {
        println!("PANIC  ⚠  {row}");
    }
    if matrix.is_empty() {
        println!("no transitions vs baseline");
    }
    for ((from, to), keys) in &matrix {
        let adverse = to == "WRONG" || (from == "RIGHT" && to != "RIGHT");
        let mut cases: BTreeMap<&str, usize> = BTreeMap::new();
        for key in keys {
            *cases.entry(case_of(key)).or_default() += 1;
        }
        let mut ranked: Vec<(&str, usize)> = cases.into_iter().collect();
        ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        let mut attribution = String::new();
        for (case, n) in ranked.iter().take(if adverse { 6 } else { 3 }) {
            let _ = write!(attribution, "  {case} {n}");
        }
        println!("{from}->{to}: {}{}{attribution}", keys.len(), if adverse { "  ⚠" } else { "" });
        if adverse {
            for key in keys.iter().take(4) {
                println!("    {key}");
            }
        }
    }

    if accept {
        std::fs::write(&baseline_path, rows.join("\n")).expect("write baseline");
        println!("baseline ACCEPTED ({})", baseline_path.display());
    }
}
