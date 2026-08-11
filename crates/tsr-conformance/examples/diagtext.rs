//! **Are the cases this suite calls "passed" actually right?**
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagtext
//! ```
//!
//! The `diagnostics` suite compares `(file, line, column, code)`. The baselines
//! carry more than that:
//!
//! ```text
//! commaOperator1.ts(1,11): error TS2695: Left side of comma operator is unused …
//! ```
//!
//! `errors_baseline::parse` reads the position and the code and drops the
//! **message**, so a diagnostic with the right code at the right position and
//! the *wrong argument* — `Cannot find name 'foo'` where upstream says `'bar'` —
//! scores as a pass. Every percentage this workstream has reported rests on that
//! comparison, and nobody has measured what it hides.
//!
//! This audits the passing cases only. For each, it renders every reported
//! diagnostic's message with its arguments and compares against the text on the
//! baseline's own line. A mismatch means the suite is crediting a case this port
//! does not actually get right.
//!
//! `checker-notes-diag2.md` §998.
use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{CaseEntry, Corpus, errors_baseline, repo_root};

/// `(file, line, column, code)` — the four fields the baseline prints.
type Key = (String, u32, u32, u32);

/// `(code, wanted text, rendered text)` for one crediting mismatch.
type Mismatch = (u32, String, String);

/// `file.ts(12,34): error TS9007: message` → `((file, line, column, code), message)`.
fn baseline_line(line: &str) -> Option<(Key, String)> {
    let (file, rest) = line.split_once('(')?;
    let (position, rest) = rest.split_once("): error TS")?;
    let (line_number, column) = position.split_once(',')?;
    let (code, message) = rest.split_once(": ")?;
    Some((
        (
            file.to_string(),
            line_number.trim().parse().ok()?,
            column.trim().parse().ok()?,
            code.trim().parse().ok()?,
        ),
        message.trim().to_string(),
    ))
}

/// `(case, mismatches)` for a case the suite currently passes.
fn measure(case: &CaseEntry) -> Option<(String, Vec<Mismatch>)> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()??;
    let expected = errors_baseline::parse(&baseline);
    if expected.is_empty() {
        return None;
    }
    let test = case.load().ok()?;
    let mut actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    let mut wanted = expected.clone();
    actual.sort_unstable();
    wanted.sort_unstable();
    // Only the cases the suite scores as passing are in question.
    if actual != wanted {
        return None;
    }
    let mut texts: BTreeMap<Key, Vec<String>> = BTreeMap::new();
    for line in baseline.lines() {
        if let Some((key, message)) = baseline_line(line) {
            texts.entry(key).or_default().push(message);
        }
    }
    let mut mismatches = Vec::new();
    for (key, rendered) in tsr_conformance::diagnostics_suite::rendered_for(&test) {
        let Some(wanted_texts) = texts.get(&key) else { continue };
        if !wanted_texts.contains(&rendered) {
            mismatches.push((key.3, wanted_texts[0].clone(), rendered));
        }
    }
    (!mismatches.is_empty()).then(|| (case.name.clone(), mismatches))
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Vec<Mismatch>)> = cases.par_iter().filter_map(measure).collect();
    let mut by_code: BTreeMap<u32, usize> = BTreeMap::new();
    for (name, mismatches) in rows.iter().take(20) {
        for (code, want, got) in mismatches.iter().take(2) {
            println!("{name}  TS{code}\n    want: {want}\n    got:  {got}");
        }
    }
    for (_, mismatches) in &rows {
        for (code, _, _) in mismatches {
            *by_code.entry(*code).or_default() += 1;
        }
    }
    let mut ranked: Vec<(u32, usize)> = by_code.into_iter().collect();
    ranked.sort_unstable_by_key(|&(code, n)| (std::cmp::Reverse(n), code));
    println!("\n{:>8}  {:>6}", "code", "lines");
    for (code, n) in ranked.iter().take(15) {
        println!("  TS{code:<6}{n:>6}");
    }
    println!("\n{} PASSING cases carry at least one wrong message", rows.len());
}
