//! `bd tsr-bxp`'s blocking measurement: is the unemitted TS2322 population a
//! **missing anchor** or a **declining relation**?
//!
//! ```text
//! TSR_ASSIGN_PROBE=1 cargo run --release -p tsr-conformance --example ts2322split
//! ```
//!
//! # The question, and why nothing answered it before
//!
//! `diagemit` says `want 2888 · have 530` for TS2322. Both halves of that gap
//! have been quoted as this workstream's target (`bd tsr-6re`) and as
//! `checker_types`' (STATUS §5's standing refusal), and **no instrument
//! separated them**. `tsr-bxp` was filed for exactly this and carries the line
//! *"Do NOT build an emitter before this runs"*.
//!
//! The split is decidable because `report_assignability_failure` is a single
//! site with three ordered gates. Under `TSR_ASSIGN_PROBE` the checker records
//! every position it was *asked about* and which gate decided it, so a wanted
//! TS2322 line is:
//!
//! | bucket | meaning | owner |
//! |---|---|---|
//! | `NEVER REACHED` | no `report_assignability_failure` call at that position | **diagnostics** — a reporting anchor |
//! | `RELATION DECLINED` | reached; the three-valued relation did not answer `NotRelated` | **`checker_types`** — members and relation completeness |
//! | `PAIR NOT REPORTABLE` | reached; one side is a type this port will not speak about | `checker_types`, narrower |
//! | `OBJECT LITERAL vs UNION` | reached; TS2353/TS2561/TS2739's machinery | a different rule |
//! | `REPORTED ELSEWHERE` | reached and reported, but at a position the baseline does not carry | a wrong position, not a missing rule |
//!
//! It measures **positions the baseline wants and we do not emit**, so it is a
//! count over the suite's own judged cases — not over baseline files, which is
//! the denominator error `bd tsr-6re` recorded against itself.

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_checker::assignreport::{
    PROBE_OBJECT_LITERAL_UNION, PROBE_PAIR_NOT_REPORTABLE, PROBE_RELATION_DECLINED, PROBE_REPORTED,
};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// §173: the node kinds at the never-reached positions, which decide whether
/// 132 cases is one build or eight.
type Anchors = Vec<(String, Option<String>)>;

/// A baseline line's key: the file it is in and its 1-based line and column.
type Position = (String, u32, u32);

/// The bucket a wanted-but-unemitted TS2322 line falls in.
#[derive(Default)]
struct Split {
    never_reached: usize,
    relation_declined: usize,
    pair_not_reportable: usize,
    object_literal_union: usize,
    reported_elsewhere: usize,
}

impl Split {
    fn add(&mut self, other: &Self) {
        self.never_reached += other.never_reached;
        self.relation_declined += other.relation_declined;
        self.pair_not_reportable += other.pair_not_reportable;
        self.object_literal_union += other.object_literal_union;
        self.reported_elsewhere += other.reported_elsewhere;
    }

    fn total(&self) -> usize {
        self.never_reached
            + self.relation_declined
            + self.pair_not_reportable
            + self.object_literal_union
            + self.reported_elsewhere
    }
}

fn main() {
    assert!(
        std::env::var("TSR_ASSIGN_PROBE").is_ok(),
        "set TSR_ASSIGN_PROBE=1 — the checker records nothing otherwise"
    );
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Split, bool, Anchors)> = cases.par_iter().filter_map(measure).collect();

    let mut lines = Split::default();
    // A case counts in a bucket if ANY of its missing TS2322 lines lands there;
    // a case can therefore appear in two, which is stated rather than hidden —
    // the line count below is the one to plan against.
    let mut cases_by_bucket: BTreeMap<&str, usize> = BTreeMap::new();
    let mut sole_obstacle_never_reached = 0usize;
    let mut sole_obstacle_relation = 0usize;
    for (_, split, sole, _) in &rows {
        lines.add(split);
        for (name, n) in [
            ("NEVER REACHED", split.never_reached),
            ("RELATION DECLINED", split.relation_declined),
            ("PAIR NOT REPORTABLE", split.pair_not_reportable),
            ("OBJECT LITERAL vs UNION", split.object_literal_union),
            ("REPORTED ELSEWHERE", split.reported_elsewhere),
        ] {
            if n > 0 {
                *cases_by_bucket.entry(name).or_default() += 1;
            }
        }
        if *sole {
            if split.never_reached > 0 {
                sole_obstacle_never_reached += 1;
            }
            if split.relation_declined > 0 {
                sole_obstacle_relation += 1;
            }
        }
    }

    println!("cases with at least one MISSING TS2322 line: {}", rows.len());
    println!("missing TS2322 lines total: {}\n", lines.total());
    println!("{:<26} {:>7}  {:>7}", "bucket", "lines", "cases");
    for (name, n) in [
        ("NEVER REACHED", lines.never_reached),
        ("RELATION DECLINED", lines.relation_declined),
        ("PAIR NOT REPORTABLE", lines.pair_not_reportable),
        ("OBJECT LITERAL vs UNION", lines.object_literal_union),
        ("REPORTED ELSEWHERE", lines.reported_elsewhere),
    ] {
        let cases = cases_by_bucket.get(name).copied().unwrap_or(0);
        println!("{name:<26} {n:>7}  {cases:>7}");
    }

    println!("\n-- of the cases blocked on TS2322 ALONE --");
    println!("  at least one line NEVER REACHED    : {sole_obstacle_never_reached}");
    println!("  at least one line RELATION DECLINED: {sole_obstacle_relation}");

    // **The number to plan against.** A case converts only when EVERY one of
    // its missing lines is emitted, so a case with one anchor-gap line and one
    // relation-gap line is not this workstream's — it is `checker_types`' with
    // an anchor prerequisite. §142's rule: a rule's yield is not its row.
    let pure_anchor = rows
        .iter()
        .filter(|(_, split, sole, _)| *sole && split.never_reached == split.total())
        .count();
    let pure_relation = rows
        .iter()
        .filter(|(_, split, sole, _)| {
            *sole && (split.relation_declined + split.pair_not_reportable) == split.total()
        })
        .count();
    let mixed = rows
        .iter()
        .filter(|(_, split, sole, _)| {
            *sole
                && split.never_reached > 0
                && (split.relation_declined + split.pair_not_reportable) > 0
        })
        .count();
    println!("\n-- and the number to plan against: EVERY missing line in one bucket --");
    println!("  wholly NEVER REACHED  (this workstream can convert alone): {pure_anchor}");
    println!("  wholly relation-gated (`checker_types` alone)            : {pure_relation}");
    println!("  MIXED (needs both)                                       : {mixed}");

    // §173. Every never-reached position, by the anchor a build would have to
    // add. Reported over the WHOLE never-reached population and again over the
    // 132 cases that convert on it alone, because a kind that dominates the
    // lines and is absent from the convertible cases is a build that measures
    // nothing.
    let mut all: BTreeMap<String, usize> = BTreeMap::new();
    let mut convertible: BTreeMap<String, usize> = BTreeMap::new();
    for (_, split, sole, anchors) in &rows {
        let pure = *sole && split.never_reached == split.total();
        for (kind, parent) in anchors {
            let label = parent
                .as_ref()
                .map_or_else(|| kind.clone(), |parent| format!("{kind} in {parent}"));
            *all.entry(label.clone()).or_default() += 1;
            if pure {
                *convertible.entry(label).or_default() += 1;
            }
        }
    }
    println!("\n-- §173: anchors at the NEVER REACHED positions --");
    println!("{:<52} {:>7} {:>12}", "anchor (node in parent)", "lines", "of which in");
    println!("{:<52} {:>7} {:>12}", "", "", "the 132");
    let mut ranked: Vec<_> = all.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (label, n) in ranked.iter().take(25) {
        let c = convertible.get(*label).copied().unwrap_or(0);
        println!("{label:<52} {n:>7} {c:>12}");
    }
    println!("  … {} distinct anchors in total", all.len());

    // **The number that decides "one build or eight".** A case converts only
    // when every anchor it needs exists, so the plannable quantity is set
    // COVERAGE over the 132, not a per-line histogram: an anchor carrying many
    // lines that are each one of several a case needs converts nothing on its
    // own. Greedy, and greedy is the right shape here because the question is
    // "what does the next build buy", asked repeatedly.
    let mut pending: Vec<std::collections::BTreeSet<String>> = rows
        .iter()
        .filter(|(_, split, sole, _)| *sole && split.never_reached == split.total())
        .map(|(_, _, _, anchors)| {
            anchors
                .iter()
                .map(|(kind, parent)| {
                    parent
                        .as_ref()
                        .map_or_else(|| kind.clone(), |parent| format!("{kind} in {parent}"))
                })
                .collect()
        })
        .collect();
    let total_pure = pending.len();
    println!("\n-- §173: cumulative conversion, greedy over the {total_pure} cases --");
    let mut built: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut cumulative = 0usize;
    for _ in 0..10 {
        let candidates: std::collections::BTreeSet<&String> =
            pending.iter().flat_map(|case| case.difference(&built)).collect();
        let mut best: Option<(String, usize)> = None;
        for anchor in candidates {
            let mut with = built.clone();
            with.insert(anchor.clone());
            let n = pending.iter().filter(|case| case.is_subset(&with)).count();
            if best.as_ref().is_none_or(|(_, b)| n > *b) {
                best = Some((anchor.clone(), n));
            }
        }
        let Some((anchor, newly)) = best else { break };
        if newly == 0 {
            println!("  (no single further anchor completes another case)");
            break;
        }
        built.insert(anchor.clone());
        cumulative += newly;
        pending.retain(|case| !case.is_subset(&built));
        println!("  +{anchor:<50} +{newly:<4} cumulative {cumulative:>4} of {total_pure}");
        if pending.is_empty() {
            break;
        }
    }
    println!("  anchors built: {}", built.len());
}

fn measure(case: &CaseEntry) -> Option<(String, Split, bool, Anchors)> {
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

    // The TS2322 lines the baseline wants and this port does not emit — as a
    // multiset difference, because a case can want two at one position.
    let mut have: HashMap<Position, usize> = HashMap::new();
    for diagnostic in actual.iter().filter(|d| d.code == 2322) {
        *have.entry((diagnostic.file.clone(), diagnostic.line, diagnostic.column)).or_default() +=
            1;
    }
    let mut missing: Vec<Position> = Vec::new();
    for diagnostic in expected.iter().filter(|d| d.code == 2322) {
        let key = (diagnostic.file.clone(), diagnostic.line, diagnostic.column);
        match have.get_mut(&key) {
            Some(n) if *n > 0 => *n -= 1,
            _ => missing.push(key),
        }
    }
    if missing.is_empty() {
        return None;
    }

    let probe = tsr_conformance::diagnostics_suite::assignability_probe_for(&test);
    let mut by_position: HashMap<Position, Vec<u8>> = HashMap::new();
    for position in probe {
        by_position
            .entry((position.file, position.line, position.column))
            .or_default()
            .push(position.verdict);
    }

    // §173: the node kinds at each position, so a never-reached one can be
    // named by the anchor a build would have to add.
    let kinds = tsr_conformance::diagnostics_suite::node_kinds_by_position_for(&test);
    let mut kinds_at: HashMap<Position, Anchors> = HashMap::new();
    for (file, line, column, kind, parent) in kinds {
        kinds_at
            .entry((file, line, column))
            .or_default()
            .push((format!("{kind:?}"), parent.map(|parent| format!("{parent:?}"))));
    }

    let mut anchors: Anchors = Vec::new();
    let mut split = Split::default();
    for key in missing {
        let Some(verdicts) = by_position.get(&key) else {
            split.never_reached += 1;
            // Several nodes share a position — `error_span` narrows a
            // declaration to its name, so the name and the declaration report
            // at the same place. The **last** registered is the innermost, and
            // the innermost is what upstream anchors on.
            if let Some(candidates) = kinds_at.get(&key)
                && let Some(anchor) = candidates.last()
            {
                anchors.push(anchor.clone());
            } else {
                anchors.push(("<no node at position>".to_string(), None));
            }
            continue;
        };
        // A position can be asked about more than once. Attribute it to the
        // gate that got FURTHEST — the one nearest to reporting — because that
        // is the gate a build would have to move.
        let best = verdicts.iter().copied().max_by_key(|verdict| match *verdict {
            PROBE_REPORTED => 4u8,
            PROBE_RELATION_DECLINED => 3,
            PROBE_PAIR_NOT_REPORTABLE => 2,
            PROBE_OBJECT_LITERAL_UNION => 1,
            _ => 0,
        });
        match best {
            Some(PROBE_REPORTED) => split.reported_elsewhere += 1,
            Some(PROBE_RELATION_DECLINED) => split.relation_declined += 1,
            Some(PROBE_PAIR_NOT_REPORTABLE) => split.pair_not_reportable += 1,
            Some(PROBE_OBJECT_LITERAL_UNION) => split.object_literal_union += 1,
            _ => split.never_reached += 1,
        }
    }

    // Is TS2322 the case's *only* missing code, with nothing extra? That is
    // `diaggap`'s single-code column, recomputed here so the two agree by
    // construction — and as a **multiset** difference, because §152 recorded
    // what a set-valued diff hides on this exact suite.
    let sole = {
        let mut remaining: BTreeMap<BaselineDiagnostic, usize> = BTreeMap::new();
        for diagnostic in &actual {
            *remaining.entry(diagnostic.clone()).or_default() += 1;
        }
        let mut missing_codes = Vec::new();
        for diagnostic in &expected {
            match remaining.get_mut(diagnostic) {
                Some(n) if *n > 0 => *n -= 1,
                _ => missing_codes.push(diagnostic.code),
            }
        }
        let extra = remaining.values().sum::<usize>();
        extra == 0 && !missing_codes.is_empty() && missing_codes.iter().all(|&c| c == 2322)
    };

    Some((case.name.clone(), split, sole, anchors))
}
