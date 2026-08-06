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
use tsr_checker::calls::counters;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// A count as a signed number, so a broken partition prints as negative
/// rather than as an enormous positive.
/// A share of a total, as a percentage.
#[allow(clippy::cast_precision_loss)]
fn share(count: u64, total: u64) -> f64 {
    if total == 0 { 0.0 } else { 100.0 * (count as f64) / (total as f64) }
}

/// The row the concentration pass ranks cases by. `TSR_FUNNEL_CONCENTRATION`
/// names it: `member-error` for the actionable typed-receiver sub-bucket,
/// anything else for the whole `callee has no object type` row.
fn concentrated(snapshot: &counters::Snapshot) -> u64 {
    match std::env::var("TSR_FUNNEL_CONCENTRATION").as_deref() {
        Ok("member-error") => snapshot.callee_member_type_error,
        Ok("receiver-generic") => snapshot.receiver_generic_member_absent,
        Ok("member-absent-intrinsic") => snapshot.member_absent_intrinsic,
        Ok("member-absent-named") => snapshot.member_absent_named_no_members,
        _ => snapshot.callee_not_anonymous,
    }
}

fn signed(count: u64) -> i64 {
    i64::try_from(count).expect("a corpus-sized count fits in an i64")
}

fn main() {
    // Before anything is checked: the switch is read once and latched.
    counters::enable();

    // One rayon thread makes the per-case deltas below meaningful; see there.
    let concentrating = std::env::var_os("TSR_FUNNEL_CONCENTRATION").is_some();
    let per_case: std::sync::Mutex<Vec<(u64, String)>> = std::sync::Mutex::new(Vec::new());

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
            // The **same entry point the types suite scores**, which is the
            // whole point of this shape. Until 2026-08-05 this example bound
            // each unit alone with no program, so it ran with no lib files
            // while the gradient ran with them; the first funnel's 12,015
            // unresolved callees were measured under that mismatch. The
            // assertions are thrown away — the counters are the output — but
            // producing them is what drives the checker over every position
            // the gradient scores.
            let before = concentrated(&counters::snapshot());
            let _ = types_producer::assertions_for_case(&parsed, &expected, false);
            // Valid **only** single-threaded: the counters are process-wide, so
            // a delta across a parallel region attributes other cases' calls to
            // this one. Hence the concentration pass runs with one rayon
            // thread, and the histogram pass — which needs no attribution —
            // runs with all of them.
            if concentrating {
                let delta = concentrated(&counters::snapshot()) - before;
                if delta > 0 {
                    per_case
                        .lock()
                        .expect("no panic held the lock")
                        .push((delta, case.name.clone()));
                }
            }
            1
        })
        .sum();

    if concentrating {
        // Rank the **files** holding the population, not the rows. On one
        // earlier row 9,999 of 11,363 lines sat in a single baseline and the
        // whole workstream evaporated; `docs/conventions.md` makes this the
        // first command, not the last.
        let mut cases = per_case.into_inner().expect("no panic held the lock");
        cases.sort_unstable_by_key(|(count, _)| std::cmp::Reverse(*count));
        let total: u64 = cases.iter().map(|(count, _)| count).sum();
        println!("concentration by case: {total} over {} cases", cases.len());
        for (count, name) in cases.iter().take(20) {
            let share = share(*count, total);
            println!("{count:>7}  {share:>5.1}%  {name}");
        }
        let top_ten: u64 = cases.iter().take(10).map(|(count, _)| count).sum();
        println!("top 10 hold {:.1}%\n", share(top_ten, total));
    }

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
    let classified = counters.callee_name_unresolved
        + counters.callee_name_type_error
        + counters.callee_name_not_object
        + counters.callee_property_receiver_error
        + counters.callee_property_receiver_typed
        + counters.callee_element_access
        + counters.callee_other_form;
    println!(
        "{:<48}{:>8}",
        "UNCLASSIFIED (callee has no object type)",
        signed(counters.callee_not_anonymous) - signed(classified)
    );
    // The three `of which` rows are a partition of `receiver is typed`, not
    // siblings of it, so they get their own control bucket.
    println!(
        "{:<48}{:>8}",
        "UNCLASSIFIED (typed receiver)",
        signed(counters.callee_property_receiver_typed)
            - signed(
                counters.callee_member_absent
                    + counters.callee_member_type_error
                    + counters.callee_member_not_object
            )
    );
    println!(
        "{:<48}{:>8}",
        "UNCLASSIFIED (no such member)",
        signed(counters.callee_member_absent)
            - signed(
                counters.member_absent_looked_up
                    + counters.member_absent_intrinsic
                    + counters.member_absent_composite
                    + counters.member_absent_named_no_members
                    + counters.member_absent_other
            )
    );
    let selection = counters.generic_candidate
        + counters.this_or_rest_parameter
        + counters.parameter_any
        + counters.spread_argument
        + counters.undecidable_pair
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
