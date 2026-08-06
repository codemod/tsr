//! Why a probe's gradient is not the suite's gradient, to the line.
//!
//! # The unexplained number
//!
//! `docs/architecture/checker-notes-recvgap.md` §8 closes with a caveat:
//!
//! > This probe re-derives the gradient as **479,060 lines = 292,217 right
//! > (61.00%) + 143,509 gap + 43,334 wrong**, against the committed
//! > `checker_types` snapshot's **61.09%**. The 0.09pp difference is **not
//! > explained**.
//!
//! It is not commit drift. `receiver_gap` was measured at `7602d6b`, and the two
//! commits between `7602d6b` and `058b4a9` that touch the checker are `3baeb70`
//! and its revert `a618e3a`, so the compiler is byte-identical at both ends —
//! `058b4a9` reads exactly the snapshot `7602d6b` wrote. The difference is in the
//! accounting, and this file is the bridge.
//!
//! An unexplained residual next to a gradient is not a rounding detail here. This
//! project decides what to build from differences of a few hundred lines, and
//! `docs/conventions.md` records the failure mode directly — *"a number can be
//! true and answer a different question"*. 106 unexplained lines in a denominator
//! is that failure with the question still unasked.
//!
//! # The three ways a probe's denominator diverges
//!
//! Every one of them is visible by reading `types_suite::compare` beside a
//! probe's loop, and none of them is visible from either alone.
//!
//! 1. **The suite counts upstream's lines; a probe counts ours.**
//!    `compare` (`types_suite.rs:98`) opens with
//!    `let total = types_baseline::assertion_count(expected)` and then iterates
//!    `expected_file.assertions` — every figure it produces is over the
//!    **baseline's** assertions. `receiver_gap` iterates `our_file`, the lines
//!    *we rendered*. Where we render more lines than upstream asserts, the probe's
//!    denominator grows and the suite's does not; where we render fewer, the
//!    suite counts baseline lines the probe never sees.
//!
//! 2. **A probe drops files the suite counts.** `receiver_gap` carries
//!    `if our_file.len() != line_ids.len() { continue; }` and
//!    `let (Some(our_file), Some(line_ids)) = … else { continue }`. Both are
//!    correct as guards — a line whose `NodeId` is unknown cannot be walked — and
//!    both remove the *whole file* from the probe's denominator while the suite
//!    keeps every baseline line in it.
//!
//! 3. **A probe drops cases the suite counts as zero.** `case.load().ok()?`
//!    returns `None`; the suite's `Judgement` for the same case is
//!    `LineTally { matched: 0, total: assertions }`. The suite is stricter, and in
//!    the direction that matters: a case that does not load is a failure, not an
//!    absence.
//!
//! (1) can push the denominator either way. (2) and (3) only ever shrink it. That
//! the net was **+106** says nothing about their sizes, which is the point of
//! measuring instead of arguing.
//!
//! # Controls
//!
//! Four, and the load-bearing one is pinned by **construction** rather than by
//! arithmetic — `docs/conventions.md` records a polarity inversion that left
//! every sum, roll-up and pairing count intact and was caught only by a bucket
//! whose value was fixed by the subject.
//!
//! - **C1, pinned by construction: `probe right - compare matched, over counted
//!   files only, must be 0.`** In a file the probe counts, the probe's test is
//!   *"the baseline's assertion at this position has this text"* and `compare`'s
//!   test is the same predicate at the same position. A match can only occur at a
//!   position below both lengths, so the two counts are the same number reached
//!   two ways, whatever the lengths are. Nothing about the corpus makes this true;
//!   the predicate does. If either side's positional alignment were inverted or
//!   off by one, this is the bucket that moves, and no sum would.
//! - **C2, pinned by construction: `no-section files with a rendered line = 0`.**
//!   A file the probe skipped because `ours.get(index)` was `None` cannot also
//!   have contributed a rendered line.
//! - **C3, arithmetic: the bridge closes.** Suite total reconstructed from the
//!   probe's total plus the four named deltas, difference 0.
//! - **C4, arithmetic: suite matched == probe right + matched-in-dropped.**
//!
//! C3 and C4 are the weak kind — they are blind to any error that moves lines
//! *between* buckets. They are printed because a bookkeeping slip is the other
//! failure, and C1/C2 are blind to that.
//!
//! # What this is for afterwards
//!
//! The rule this leaves behind is cheap and standing: **a probe that quotes a
//! gradient must reconcile its denominator against `assertion_count`, or quote
//! shares of its own population and say so.** `receiver_gap` does the second
//! thing correctly for every figure on its page — every share there is taken over
//! the two rows, whose denominator it computes itself — and then quotes an
//! absolute in §8. This file is what that §8 needed.
//!
//! Reproduce:
//!
//! ```text
//! cargo run --release -p tsr-conformance --example reconcile
//! ```

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::types_baseline::{self, FileTypes};
use tsr_conformance::{Corpus, repo_root, types_producer, types_suite};

/// Every quantity the bridge needs, summed over cases.
#[derive(Default, Clone)]
struct Bridge {
    /// Cases the suite judged (skips already removed).
    cases: usize,

    // ---- the suite's own numbers, taken from `types_suite::compare` ----
    /// `assertion_count(expected)`, summed. The `checker_types` denominator.
    suite_total: usize,
    /// `compare(...).lines.matched`, summed. The `checker_types` numerator.
    suite_matched: usize,

    // ---- a `receiver_gap`-shaped probe's numbers ----
    /// Rendered lines in files the probe counts.
    probe_total: usize,
    /// Of those, the ones whose text matches the baseline at the same position.
    probe_right: usize,

    // ---- the four deltas ----
    /// Baseline lines in cases that did not load. Probe: invisible. Suite: 0/total.
    load_fail_baseline: usize,
    /// Cases that did not load.
    load_fail_cases: usize,
    /// Baseline lines in files for which we rendered no section at all.
    no_section_baseline: usize,
    /// Files for which we rendered no section.
    no_section_files: usize,
    /// Baseline lines in files dropped by the id-length guard.
    id_guard_baseline: usize,
    /// Rendered lines in those same files — in the probe's total, not the suite's.
    id_guard_rendered: usize,
    /// Files dropped by the id-length guard.
    id_guard_files: usize,
    /// In counted files: rendered lines beyond the baseline's count.
    surplus: usize,
    /// In counted files: baseline lines beyond ours.
    deficit: usize,

    // ---- controls ----
    /// C1: per counted file, `probe right - compare matched`. Must be 0.
    c1_right_disagreement: i128,
    /// C2: rendered lines under a file the probe called "no section". Must be 0.
    c2_no_section_with_lines: usize,
    /// Suite-matched lines inside anything the probe dropped.
    dropped_matched: usize,
    /// Lines we answered `error` on where the baseline says `error` too.
    ///
    /// A gap/right classifier that tests `type_string == "error"` **before** it
    /// tests the baseline puts these in `gap`. They are right answers.
    error_but_matching: usize,
}

impl Bridge {
    fn merge(&mut self, other: &Self) {
        self.cases += other.cases;
        self.suite_total += other.suite_total;
        self.suite_matched += other.suite_matched;
        self.probe_total += other.probe_total;
        self.probe_right += other.probe_right;
        self.load_fail_baseline += other.load_fail_baseline;
        self.load_fail_cases += other.load_fail_cases;
        self.no_section_baseline += other.no_section_baseline;
        self.no_section_files += other.no_section_files;
        self.id_guard_baseline += other.id_guard_baseline;
        self.id_guard_rendered += other.id_guard_rendered;
        self.id_guard_files += other.id_guard_files;
        self.surplus += other.surplus;
        self.deficit += other.deficit;
        self.c1_right_disagreement += other.c1_right_disagreement;
        self.c2_no_section_with_lines += other.c2_no_section_with_lines;
        self.dropped_matched += other.dropped_matched;
        self.error_but_matching += other.error_but_matching;
    }
}

/// `compare`, restricted to one file section, so C1's right-hand side comes from
/// the suite's own function rather than from a re-implementation of it.
/// Signed difference of two counts, without a lossy cast. Same helper shape as
/// `rank_board`'s, so the two probes' controls are written the same way.
fn delta(left: usize, right: usize) -> i128 {
    i128::try_from(left).unwrap_or_default() - i128::try_from(right).unwrap_or_default()
}

fn matched_in_file(expected: &FileTypes, ours: &FileTypes) -> usize {
    let one_expected = std::slice::from_ref(expected);
    let one_ours = std::slice::from_ref(ours);
    types_suite::compare(one_expected, one_ours).lines.matched
}

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<Bridge> {
    // The suite's skips, skip for skip, and in the suite's order — asking for the
    // plain `.types` first would send all 2,032 configuration-varied cases into
    // the "no baseline" bucket (`types_suite.rs:184`).
    if case.has_varied_types() || case.has_known_divergence() {
        return None;
    }
    let text = case.expected_types()?;
    let expected = types_baseline::parse(&text);
    let assertions = types_baseline::assertion_count(&expected);
    if assertions == 0 {
        return None;
    }

    let mut bridge = Bridge { cases: 1, suite_total: assertions, ..Bridge::default() };

    // The suite's own third failure mode: a case that does not load is judged,
    // with every baseline line counted and none matched.
    let Ok(parsed) = case.load() else {
        bridge.load_fail_cases = 1;
        bridge.load_fail_baseline = assertions;
        return Some(bridge);
    };

    let arena = tsr_core::Arena::new();
    let (_program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);

    // The suite's numbers, from the suite's function on the suite's input shape.
    let ours_ft: Vec<FileTypes> = ours
        .iter()
        .zip(&expected)
        .map(|(rendered, expected_file)| {
            types_producer::to_file_types(&expected_file.file, rendered)
        })
        .collect();
    let comparison = types_suite::compare(&expected, &ours_ft);
    bridge.suite_matched = comparison.lines.matched;

    for (index, expected_file) in expected.iter().enumerate() {
        let baseline_lines = expected_file.assertions.len();

        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else {
            bridge.no_section_files += 1;
            bridge.no_section_baseline += baseline_lines;
            // C2 is pinned by construction: the arm that fired is exactly
            // `ours.get(index).is_none()`, so there is no rendered line to count.
            if let Some(rendered) = ours.get(index) {
                bridge.c2_no_section_with_lines += rendered.len();
            }
            bridge.dropped_matched +=
                ours_ft.get(index).map_or(0, |o| matched_in_file(expected_file, o));
            continue;
        };

        if our_file.len() != line_ids.len() {
            bridge.id_guard_files += 1;
            bridge.id_guard_baseline += baseline_lines;
            bridge.id_guard_rendered += our_file.len();
            bridge.dropped_matched +=
                ours_ft.get(index).map_or(0, |o| matched_in_file(expected_file, o));
            continue;
        }

        // A counted file. The probe's denominator is what *we* rendered.
        bridge.probe_total += our_file.len();
        bridge.surplus += our_file.len().saturating_sub(baseline_lines);
        bridge.deficit += baseline_lines.saturating_sub(our_file.len());

        let mut right = 0usize;
        for (position, assertion) in our_file.iter().enumerate() {
            if expected_file.assertions.get(position).is_some_and(|b| b.text == assertion.line()) {
                right += 1;
                if assertion.type_string == "error" {
                    bridge.error_but_matching += 1;
                }
            }
        }
        bridge.probe_right += right;

        // C1. Same predicate, same positions, reached two ways.
        let via_compare = ours_ft.get(index).map_or(0, |o| matched_in_file(expected_file, o));
        bridge.c1_right_disagreement += delta(right, via_compare);
    }

    Some(bridge)
}

#[allow(clippy::cast_precision_loss)]
fn print(b: &Bridge) {
    let pct = |n: usize, d: usize| n as f64 / d.max(1) as f64 * 100.0;

    println!("cases judged: {}\n", b.cases);

    println!("THE TWO GRADIENTS");
    println!(
        "  suite  (baseline lines)   {:>7} / {:>7}   {:.2}%",
        b.suite_matched,
        b.suite_total,
        pct(b.suite_matched, b.suite_total)
    );
    println!(
        "  probe  (rendered lines)   {:>7} / {:>7}   {:.2}%",
        b.probe_right,
        b.probe_total,
        pct(b.probe_right, b.probe_total)
    );
    println!(
        "  difference                {:>7}   {:>7}\n",
        delta(b.suite_matched, b.probe_right),
        delta(b.suite_total, b.probe_total)
    );

    println!("THE DENOMINATOR BRIDGE  (probe total -> suite total)");
    println!("  {:<52} {:>8}", "probe total (rendered lines, counted files)", b.probe_total);
    println!(
        "  {:<52} {:>8}",
        format!("- surplus: we rendered more than upstream asserts"),
        b.surplus
    );
    println!("  {:<52} {:>8}", "+ deficit: upstream asserts more than we rendered", b.deficit);
    println!(
        "  {:<52} {:>8}",
        format!("+ baseline lines in {} no-section file(s)", b.no_section_files),
        b.no_section_baseline
    );
    println!(
        "  {:<52} {:>8}",
        format!("+ baseline lines in {} id-guard file(s)", b.id_guard_files),
        b.id_guard_baseline
    );
    println!(
        "  {:<52} {:>8}",
        format!("  (those files also held rendered lines, not counted)",),
        b.id_guard_rendered
    );
    println!(
        "  {:<52} {:>8}",
        format!("+ baseline lines in {} case(s) that did not load", b.load_fail_cases),
        b.load_fail_baseline
    );
    let reconstructed = delta(b.probe_total, b.surplus)
        + i128::try_from(b.deficit).unwrap_or_default()
        + i128::try_from(b.no_section_baseline).unwrap_or_default()
        + i128::try_from(b.id_guard_baseline).unwrap_or_default()
        + i128::try_from(b.load_fail_baseline).unwrap_or_default();
    println!("  {:<52} {:>8}", "= reconstructed suite total", reconstructed);
    println!("  {:<52} {:>8}\n", "  actual suite total", b.suite_total);

    println!("CONTROLS");
    println!(
        "  C1  probe right - compare matched, counted files    {:>8}   (0, pinned by construction: same predicate, same positions)",
        b.c1_right_disagreement
    );
    println!(
        "  C2  rendered lines under a no-section file          {:>8}   (0, pinned by construction: the arm IS `ours.get(i).is_none()`)",
        b.c2_no_section_with_lines
    );
    println!(
        "  C3  reconstructed suite total - actual              {:>8}   (0, arithmetic)",
        reconstructed - i128::try_from(b.suite_total).unwrap_or_default()
    );
    println!(
        "  C4  suite matched - (probe right + dropped matched) {:>8}   (0, arithmetic)",
        delta(b.suite_matched, b.probe_right + b.dropped_matched)
    );
    println!("      of which matched inside dropped files/cases   {:>8}\n", b.dropped_matched);

    println!("THE NUMERATOR BRIDGE  (why `receiver_gap` read 292,217)");
    println!("  {:<52} {:>8}", "right, testing the baseline first", b.probe_right);
    println!(
        "  {:<52} {:>8}",
        "- lines answering `error` that the baseline also says", b.error_but_matching
    );
    println!(
        "  {:<52} {:>8}",
        "= right, testing `type_string == \"error\"` first",
        b.probe_right - b.error_but_matching
    );
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let bridges: Vec<Bridge> = cases.par_iter().filter_map(measure).collect();
    let mut total = Bridge::default();
    for bridge in &bridges {
        total.merge(bridge);
    }
    print(&total);
}
