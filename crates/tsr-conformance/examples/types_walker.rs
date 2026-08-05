//! How far the `.types` walker agrees with upstream — **types excluded**.
//!
//! `cargo run -p tsr-conformance --example types_walker --release`
//!
//! A wrong walker and a wrong checker are indistinguishable in the line
//! gradient: both simply fail to match. This isolates the walker by comparing
//! only the *expression text* of each assertion, testing whether upstream's line
//! starts with `{our text} : `. That is sound even though the line itself cannot
//! be split — the expression may contain `" : "` — because a prefix test needs no
//! split.
//!
//! Until this reads high, the `checker_types` gradient says nothing about the
//! checker. See `docs/architecture/checker-oracle.md`.

use std::collections::BTreeMap;

use rayon::prelude::*;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// One case's first divergence: case name, upstream's line, our text.
type Divergence = (String, String, String);

/// What one case contributed.
#[derive(Default, Clone, Copy)]
struct Tally {
    cases: usize,
    matched: usize,
    expected: usize,
    produced: usize,
    /// Cases where our assertion count equals upstream's.
    same_count: usize,
    /// Cases where every position matched.
    perfect: usize,
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let results: Vec<(Tally, Option<Divergence>)> = cases
        .par_iter()
        .map(|case| {
            let mut tally = Tally::default();
            if case.has_varied_types() || case.has_known_divergence() {
                return (tally, None);
            }
            let Some(text) = case.expected_types() else { return (tally, None) };
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return (tally, None);
            }
            let Ok(parsed) = case.load() else { return (tally, None) };

            // Per baseline section, in the baseline's order, so position `i` on
            // one side is position `i` on the other.
            let mut ours = Vec::new();
            for expected_file in &expected {
                let Some(unit) =
                    parsed.files.iter().find(|u| crate_same_unit(&u.name, &expected_file.file))
                else {
                    ours.push(Vec::new());
                    continue;
                };
                let kind = tsr_parser::ScriptKind::from_file_name(&unit.name);
                if kind == tsr_parser::ScriptKind::Json {
                    ours.push(Vec::new());
                    continue;
                }
                let arena = tsr_core::Arena::new();
                let options = tsr_parser::ParseOptions {
                    jsdoc: false,
                    ..tsr_parser::ParseOptions::for_file(&unit.name)
                };
                let file = tsr_parser::parse_with_options(&arena, &unit.content, options);
                // The type is irrelevant here and is stubbed, which is the point:
                // this measures node selection and text extraction alone.
                let assertions = types_producer::assertions_for_file(
                    &tsr_ast::Node::SourceFile(file.source_file),
                    &unit.content,
                    &file.nodes,
                    &file.node_map,
                    |_| String::new(),
                );
                ours.push(assertions);
            }

            let agreement = types_producer::walker_agreement(&expected, &ours);
            tally.cases = 1;
            tally.matched = agreement.matched;
            tally.expected = agreement.expected;
            tally.produced = agreement.produced;
            tally.same_count = usize::from(agreement.produced == agreement.expected);
            tally.perfect =
                usize::from(agreement.matched == agreement.expected && tally.same_count == 1);

            // A sample mismatch, for the first differing position.
            let mut sample = None;
            'outer: for (index, expected_file) in expected.iter().enumerate() {
                let Some(our_file) = ours.get(index) else { continue };
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let got = our_file.get(position).map(|a| a.text.clone()).unwrap_or_default();
                    if !want.text.starts_with(&format!("{got} : ")) {
                        sample = Some((case.name.clone(), want.text.clone(), got));
                        break 'outer;
                    }
                }
            }
            (tally, sample)
        })
        .collect();

    let mut total = Tally::default();
    let mut samples: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (tally, sample) in results {
        total.cases += tally.cases;
        total.matched += tally.matched;
        total.expected += tally.expected;
        total.produced += tally.produced;
        total.same_count += tally.same_count;
        total.perfect += tally.perfect;
        if let Some((name, want, got)) = sample
            && samples.len() < 20
        {
            samples.insert(name, (want, got));
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let pct = |n: usize, d: usize| if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 };

    println!("cases judged:            {}", total.cases);
    println!("assertion lines upstream: {}", total.expected);
    println!("assertion lines we emit:  {}", total.produced);
    println!(
        "text agreement:           {}/{} ({:.2}%)",
        total.matched,
        total.expected,
        pct(total.matched, total.expected)
    );
    println!(
        "cases with the right count: {}/{} ({:.2}%)",
        total.same_count,
        total.cases,
        pct(total.same_count, total.cases)
    );
    println!(
        "cases matching every line:  {}/{} ({:.2}%)",
        total.perfect,
        total.cases,
        pct(total.perfect, total.cases)
    );

    println!("\nfirst divergence in {} sampled cases:", samples.len());
    for (name, (want, got)) in samples.iter().take(20) {
        println!("  {name}\n    upstream: {want}\n    ours:     {got}");
    }
}

/// Same unit-vs-baseline-section matching the other suites use.
fn crate_same_unit(unit: &str, baseline: &str) -> bool {
    let normalise = |path: &str| path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    normalise(unit) == normalise(baseline)
}
