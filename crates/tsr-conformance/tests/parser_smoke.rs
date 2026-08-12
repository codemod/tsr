//! A fast corpus-backed parser gate, runnable under `cargo test`.
//!
//! # The gap this closes, and what it cost to find
//!
//! `parser_typescript` sits at 100% and is the instrument that would catch any
//! catastrophic parser regression — but it only runs under
//! `cargo run -p tsr-conformance --bin coverage`, a multi-minute release job.
//! Nothing in `cargo test --workspace` reads the corpus at all.
//!
//! §233 demonstrated what that costs. Building `new.target`, a dropped
//! `next_token()` left the `new` keyword unconsumed on the ordinary path and
//! **lost 396 `checker_types` cases**. The edit compiled, `cargo clippy` was
//! clean, and the parser's own unit tests were **green** — 114 of them. The
//! only instrument that noticed was the full conformance run, which is the
//! slow one and the one a hurried session skips.
//!
//! A gate that reports success while the thing it guards is destroyed is worse
//! than no gate, because it is *believed*. This test exists so the fast gate
//! can fail.
//!
//! # Why a sample, and why this sample
//!
//! The full suite parses every case in the corpus; in a debug build that is far
//! too slow for `cargo test`. This takes a **deterministic** slice — every
//! `STRIDE`th case, so the set is stable across runs and across machines — and
//! asserts the property `parser_typescript` asserts: **a file upstream reports
//! no errors for must parse with no diagnostics.**
//!
//! Determinism matters more than coverage here. A random sample would make a
//! failure hard to reproduce, and the failures this is for are not subtle: the
//! §233 regression broke roughly a case in twenty-four, which any sample of
//! this size catches many times over. This is a smoke alarm, not a survey —
//! `parser_typescript` remains the measurement.

use tsr_conformance::corpus::Corpus;
use tsr_conformance::repo_root;

/// Every `STRIDE`th case. Chosen so the sample is a few hundred files: large
/// enough that a broad regression cannot slip through, small enough to stay
/// inside a debug-build `cargo test`.
const STRIDE: usize = 24;

/// How many failures to name before giving up. A real parser regression breaks
/// hundreds at once, and a hundred-line panic message helps nobody; the count
/// is what tells you which kind of failure you have.
const REPORTED: usize = 10;

#[test]
fn a_sample_of_error_free_corpus_files_parses_with_no_diagnostics() {
    let corpus = Corpus::from_repo_root(&repo_root());
    if !corpus.is_available() {
        // The submodule is not checked out. Skipping is right — this is a
        // regression alarm, not a completeness claim — but say so rather than
        // passing silently, since a silent pass here is the exact failure the
        // test exists to prevent.
        eprintln!(
            "SKIPPED: corpus unavailable; run `git submodule update --init --recursive`. \
             This is not a pass."
        );
        return;
    }
    let cases = corpus.discover().expect("discovering cases");
    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for case in cases.iter().step_by(STRIDE) {
        // The same three exclusions `suites::Parser` makes, for the same
        // reasons: no baseline means nothing to judge against, a `.diff`
        // baseline means upstream itself diverges, and a case that EXPECTS
        // diagnostics cannot tell ours from the checker's.
        if !case.has_any_baseline() || case.has_known_divergence() || case.has_varied_errors() {
            continue;
        }
        match case.expected_errors() {
            Ok(None) => {}
            _ => continue,
        }
        let Ok(loaded) = case.load() else { continue };
        let arena = tsr_core::Arena::new();
        for unit in &loaded.files {
            // **Route by extension, as the real suite does.** The first draft
            // called `tsr_parser::parse`, which defaults to `ScriptKind::TypeScript`,
            // and promptly "found" nine diagnostics in every `.tsx` file and one
            // in a `package.json`. Those were the test's bug, not the parser's —
            // a gate that fails for its own reasons is no better than one that
            // passes for them.
            let kind = match unit.name.rsplit_once('.').map(|(_, ext)| ext) {
                Some("tsx" | "jsx") => tsr_parser::ScriptKind::Tsx,
                Some("json") => continue,
                _ => tsr_parser::ScriptKind::TypeScript,
            };
            let parsed = tsr_parser::parse_with_script_kind(&arena, &unit.content, kind);
            checked += 1;
            if !parsed.diagnostics.is_empty() && failures.len() < REPORTED {
                failures.push(format!(
                    "{} :: {} — {} diagnostic(s), first: {}",
                    case.name,
                    unit.name,
                    parsed.diagnostics.len(),
                    parsed.diagnostics.first().map_or_else(String::new, tsr_diagnostics::Diagnostic::text),
                ));
            }
        }
    }

    assert!(checked > 100, "the sample collapsed to {checked} files; STRIDE or the corpus moved");
    assert!(
        failures.is_empty(),
        "{} of {checked} sampled files failed to parse cleanly (first {} shown):\n{}",
        failures.len(),
        failures.len().min(REPORTED),
        failures.join("\n")
    );
}
