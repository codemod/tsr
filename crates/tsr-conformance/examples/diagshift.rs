//! Diagnostics this port reports **at the wrong position** — a code that is both
//! missing somewhere in a case and extra somewhere else in the same case.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagshift
//! ```
//!
//! # Why this shape needs its own view
//!
//! §887 was one rule producing two defects: TS2397 on the `class undefined`,
//! where TS2414 had already spoken, and nothing on the `namespace undefined`,
//! where it was the only speaker. `diagmissing` saw the gap, `extraonly` saw the
//! extra, `diagdup` saw the extra sitting on a right position — and **none of
//! the three says the two are the same rule reporting in the wrong place**.
//!
//! A displaced diagnostic is worth separating from a missing one because it
//! costs *two* lines and is usually one edit: the rule already fires, already
//! computes the message, and is wrong only about which node it belongs on.
//!
//! # What a row does and does not say
//!
//! Each missing line is paired with the **first** extra of the same code, so a
//! rule that fires *once* where eight are wanted prints eight rows naming the
//! same reported position. That is not eight displacements; it is one wrong
//! line and seven gaps, and `externalModuleImmutableBindings` is exactly that
//! shape (§888). The row means *"this code is on both sides of this case's
//! difference"* — a rule that reports the code and gets a position wrong — and
//! the counts on either side have to be read from the case.
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .expect("sizing the corpus thread pool");
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Vec<String>)> = cases.par_iter().filter_map(measure).collect();
    let mut lines = 0;
    for (name, shifts) in &rows {
        for shift in shifts {
            println!("{name}  {shift}");
            lines += 1;
        }
    }
    println!("cases with a displaced diagnostic: {}", rows.len());
    println!("codes displaced: {lines}");
}

/// The codes that appear on **both** sides of this case's difference: wanted at
/// a position this port does not report, and reported at a position the
/// baseline does not want.
fn measure(case: &CaseEntry) -> Option<(String, Vec<String>)> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return None;
    }
    let test = case.load().ok()?;
    let actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    let missing: Vec<&BaselineDiagnostic> =
        expected.iter().filter(|d| !actual.contains(d)).collect();
    let extra: Vec<&BaselineDiagnostic> = actual.iter().filter(|d| !expected.contains(d)).collect();
    let mut shifts: Vec<String> = Vec::new();
    for want in &missing {
        for have in &extra {
            if want.code == have.code {
                shifts.push(format!(
                    "TS{} wanted at {}({},{}), reported at ({},{})",
                    want.code, want.file, want.line, want.column, have.line, have.column
                ));
                break;
            }
        }
    }
    if shifts.is_empty() { None } else { Some((case.name.clone(), shifts)) }
}
