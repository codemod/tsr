//! The port's diagnostics and the baseline's, side by side, for one named case.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagcase -- conformance/umd1
//! ```
//!
//! `diagmissing.rs` names the cases; this reads one. It calls the suite's own
//! `reported_for`, so what it prints is what the suite judges — the same
//! probe-and-build identity `diag2307.rs` is built on.
use tsr_conformance::{Corpus, errors_baseline, repo_root};

fn main() {
    let name = std::env::args().nth(1).expect("case name");
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let case = cases.iter().find(|c| c.name == name).expect("case not found");
    let baseline = case.expected_errors().ok().flatten();
    let expected = baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    let test = case.load().expect("load");
    let actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    println!("-- expected --");
    for d in &expected {
        println!("  {}({},{}) TS{}", d.file, d.line, d.column, d.code);
    }
    println!("-- actual --");
    for d in &actual {
        println!("  {}({},{}) TS{}", d.file, d.line, d.column, d.code);
    }
}
