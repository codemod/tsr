//! Where a call stops: the attribution histogram for overload selection.
//!
//! `cargo run -p tsr-conformance --example overload_funnel --release`
//!
//! Overload selection (`6c55f29`) was predicted to move "low hundreds" of
//! assertion lines and moved **48, across 8 cases**. Its author then classified
//! the 779 overloaded call sites from the baseline sources and found that
//! `SELECTABLE` — the parameter-side guard `bd tsr-6v7` proposes widening —
//! admits ~198 of them. 198 admitted against 8 landed leaves at least three
//! quarters of the attenuation unattributed, and the author flagged its own
//! classification as a regex over single-line signatures, with two counts of
//! generics that disagreed 84 against 162.
//!
//! This probe replaces that classification with counters inside the code, so
//! the question "would widening `SELECTABLE` be worth anything" is answered by
//! what the checker does rather than by what the source looks like. It runs the
//! `.types` producer over the same corpus population the types suite uses and
//! prints [`tsr_checker::calls::counters`].
//!
//! # What the rows mean
//!
//! Two funnels, each partitioning its own denominator:
//!
//! - **call expressions checked** — every `check_call_expression` entry. The
//!   indented rows under it are where a call stopped *before* selection; a call
//!   that reaches an overload set is counted in `overload sets reaching
//!   choose_overload`, which is the second denominator.
//! - **overload sets reaching `choose_overload`** — the indented rows under it
//!   are the five gates plus the outcome, in the order the code tests them.
//!
//! `UNATTRIBUTED` is printed for both, unconditionally and including at zero:
//! it is the denominator minus its buckets, zero by construction, and non-zero
//! only if the counters have stopped partitioning the code they instrument.
//!
//! The one row that is not a sibling is `of which the return type is error`, a
//! **subset** of `SELECTED`. It is the measurement that separates "selection
//! failed" from "selection succeeded and bought nothing", which are the same
//! number in the gradient and different items on the board.
//!
//! # The shadowing, which is not an artefact to be fixed
//!
//! Arguments are checked after the parameter gate, so a site rejected for its
//! parameters is never classified on its arguments. Checking arguments earlier
//! would call `check_expression` on expressions the checker does not otherwise
//! visit and perturb the caches feeding the assertion lines — a behaviour
//! change, which this probe is not allowed. So the argument row reads
//! *"among the sites the parameter gate admitted"*, and the correct way to size
//! the argument gate against a widened `SELECTABLE` is to widen it and re-run
//! this, not to read the two rows as independent causes.

use rayon::prelude::*;
use tsr_ast::Node;
use tsr_checker::calls::counters;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// A count as a signed number, so a broken partition prints as negative
/// rather than as an enormous positive.
fn signed(count: u64) -> i64 {
    i64::try_from(count).expect("a corpus-sized count fits in an i64")
}

fn main() {
    // Before anything is checked: the switch is read once and latched.
    counters::enable();

    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let counted_cases: usize = cases
        .par_iter()
        .map(|case| {
            if case.has_varied_types() || case.has_known_divergence() {
                return 0;
            }
            let Some(text) = case.expected_types() else { return 0 };
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return 0;
            }
            let Ok(parsed) = case.load() else { return 0 };

            for expected_file in &expected {
                let Some(unit) = parsed.files.iter().find(|u| {
                    tsr_conformance::binder_suite::same_unit(&u.name, &expected_file.file)
                }) else {
                    continue;
                };
                if tsr_parser::ScriptKind::from_file_name(&unit.name)
                    == tsr_parser::ScriptKind::Json
                {
                    continue;
                }
                let arena = tsr_core::Arena::new();
                let options = tsr_parser::ParseOptions {
                    jsdoc: false,
                    ..tsr_parser::ParseOptions::for_file(&unit.name)
                };
                let file = tsr_parser::parse_with_options(&arena, &unit.content, options);
                let bound = tsr_binder::bind(
                    file.source_file,
                    &file.nodes,
                    tsr_binder::FileInfo { name: &unit.name, text: &unit.content },
                );
                let mut checker = tsr_checker::Checker::new(&bound, &file.nodes, &file.node_map);
                // The assertions are thrown away — this probe reads the
                // counters, not the lines. Producing them is what drives the
                // checker over every position the gradient scores, which is the
                // only way the funnel's denominator matches the corpus the
                // types suite reports on.
                types_producer::assertions_for_file(
                    &Node::SourceFile(file.source_file),
                    &unit.content,
                    &file.nodes,
                    &file.node_map,
                    |id| {
                        types_producer::type_at_location(
                            &mut checker,
                            &bound,
                            &file.nodes,
                            &file.node_map,
                            id,
                        )
                    },
                );
            }
            1
        })
        .sum();

    let counters = counters::snapshot();
    println!("cases: {counted_cases}\n");
    for (label, value) in counters.rows() {
        println!("{label:<48}{value:>8}");
    }

    // Both control buckets, printed at zero as well: a non-zero reading means
    // the counters no longer partition the code, and the histogram above is
    // not a histogram.
    let pre_selection = counters.optional_chain
        + counters.callee_not_anonymous
        + counters.callee_no_signatures
        + counters.callee_zero_signatures
        + counters.single_candidate
        + counters.overload_sets;
    println!(
        "{:<48}{:>8}",
        "UNATTRIBUTED (before selection)",
        signed(counters.call_expressions) - signed(pre_selection)
    );
    let selection = counters.generic_candidate
        + counters.this_or_rest_parameter
        + counters.parameter_not_selectable
        + counters.spread_argument
        + counters.argument_not_selectable
        + counters.arity_no_match
        + counters.no_assignable_candidate
        + counters.ambiguous_return
        + counters.selected;
    println!(
        "{:<48}{:>8}",
        "UNATTRIBUTED (within selection)",
        signed(counters.overload_sets) - signed(selection)
    );
}
