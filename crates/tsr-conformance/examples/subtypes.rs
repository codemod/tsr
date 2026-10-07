//! How much `removeSubtypes` would move — `bd tsr-eak`.
//!
//! # The registered instruction, and why it forbids the obvious sum
//!
//! `bd tsr-eak` names five rows that `UnionReductionSubtype` blocks: `||` 358,
//! `??` 96, `ConditionalExpression` 295, the `ArrayLiteral` object-reduction
//! guard 355, and return inference from a multi-return body. It also registers
//! that the item must **not** be sized by adding those up — they live in four
//! files, they do not ship together, and a case needing two is finished by
//! neither (`docs/conventions.md`, *"do not sum unrelated sub-items to clear a
//! bar"*). The instruction was:
//!
//! > For every union this port builds, does `removeSubtypes` drop a
//! > constituent, and does the baseline agree with the reduced form?
//!
//! # Reading upstream first narrows the question by an order of magnitude
//!
//! `removeSubtypes` (`checker.go:25934`) only ever considers a constituent for
//! removal when it is **`StructuredOrInstantiable`** (`:25955`), and upstream
//! says why in a comment on the line above:
//!
//! > We assume that redundant primitive types have already been removed from
//! > the types array and that there are no any and unknown types in the array.
//! > Thus, the only possible supertypes for primitive types are empty object
//! > types, and if none of those are present we can exclude primitive types
//! > from the subtype check.
//!
//! `TypeFlagsStructuredOrInstantiable` is `Object | Union | Intersection |
//! TypeParameter | IndexedAccess | Conditional | Substitution | Index |
//! TemplateLiteral | StringMapping` (`types.go:482`–`:487`).
//!
//! **So `UnionReductionLiteral` — which this port has — and
//! `UnionReductionSubtype` can only disagree on a union carrying at least
//! *one* structured constituent.** A union of pure primitives reduces
//! identically under both.
//!
//! ## The "at least two" version of that sentence was wrong, and C1 caught it
//!
//! This file first said **two**, reasoning that a removal needs a structured
//! source *and* a structured target. It does not. The gate at `:25955` is
//! **per source**, and the target loop at `:25984` ranges over *every other
//! constituent* whatever its flags — so `T extends string` in `T | string` is a
//! structured source removed against a primitive target, with one structured
//! constituent in the union. Upstream's own comment three lines further down
//! is about exactly that case.
//!
//! C1 was registered as *"a union with fewer than two structured constituents
//! cannot change, expect 0"* and read **61**. The control was pinned to
//! upstream's gate rather than to this file's arithmetic, which is the only
//! reason it could fire: had it been written as "the buckets sum", it would
//! have passed and the population would have been understated by the size of
//! the one-structured bucket.
//!
//! It is the same error `binary.rs`'s `getTypeFacts` comment made — read
//! upstream, infer a rule, state the inference as upstream's — one cycle later
//! and with a control on it.
//!
//! # What is measured exactly, and what is not
//!
//! **Measured exactly (§A).** Every aligned assertion line whose type this port
//! already answers as a union. Cross-tabbed: does the line match the baseline
//! today, and does it carry a structured constituent? The second column is the
//! one that matters, because **a union we answer *correctly* today and that
//! carries a structured constituent is a line `removeSubtypes` could break.** That is the qualified-naming lesson applied before the build rather
//! than after: *an accuracy bar on the target row licenses nothing when the
//! mechanism fires wider*, and unions are everywhere.
//!
//! **Measured exactly (§B).** Among the lines with a structured constituent,
//! whether the *baseline* keeps every constituent our union has, and — the
//! question that actually decides the item — **why** the ones we get wrong are
//! wrong. `family()` partitions those on the two strings alone, because
//! "something was collapsed" lumps subtype reduction together with narrowing,
//! constituent order and parenthesisation, and only one of those four is this
//! item.
//!
//! §B's `keeps_all` column has a known defect and is not quoted: a union that
//! prints as a **name** rather than as its constituents — `boolean`, which is
//! `false | true` — reads as "collapsed" against a baseline saying `boolean`.
//! The `family()` partition below is over lines we get *wrong*, where that
//! cannot happen, which is why the verdict rests on it.
//!
//! **Not measured (§C).** The gap rows. Their answers cannot be computed
//! without the build, and the probe prints their populations without a
//! conversion beside them, which is what a population is.
//!
//! # The proxy, declared
//!
//! Upstream removes on `strictSubtypeRelation`. This port has one `Relation`
//! variant, `Assignable`, which is **weaker** — it relates strictly more pairs —
//! so using it over-removes and therefore over-states both the conversions and
//! the breakage. Every figure computed through it is labelled `assignable-proxy`
//! and is an **upper bound in both directions**, never a point estimate.
//! `docs/conventions.md`: a proxy agreeing with what it proxies is not evidence
//! the proxy works, so nothing here is quoted without that label.
//!
//! # Controls
//!
//! - **C1, construction.** A union with **no** structured constituent cannot
//!   change under `removeSubtypes`: upstream's gate at `:25955` admits a source
//!   only when it is `StructuredOrInstantiable` *or* `hasEmptyObject`, and
//!   `hasEmptyObject` itself requires an object-typed constituent. So the
//!   `would-change` count in that bucket must be **0**. It is pinned by
//!   upstream's gate, not by this file's arithmetic. **It fired once already**,
//!   at 61, against the "fewer than two" partition this file first used — see
//!   the module docs.
//! - **C2, arithmetic.** The four cross-tab cells sum to the union population.
//! - **C3, frozen.** `array_literals.rs`'s guard population was published at
//!   602 own-root lines with 355 in one case
//!   (`docs/architecture/checker-notes-armsplit.md` §5, at `c329b9f`). §C
//!   recounts it by a different route.
//!
//! Run: `cargo run --release -p tsr-conformance --example subtypes`

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, SyntaxKind};
use tsr_checker::flags::TypeFlags;
use tsr_checker::types::TypeData;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// `TypeFlagsStructuredOrInstantiable` (`types.go:487`), minus `Union` —
/// a union's constituents are flattened, so a constituent is never one.
fn is_structured(flags: TypeFlags) -> bool {
    flags.intersects(
        TypeFlags::OBJECT
            | TypeFlags::INTERSECTION
            | TypeFlags::TYPE_PARAMETER
            | TypeFlags::INDEXED_ACCESS
            | TypeFlags::CONDITIONAL
            | TypeFlags::SUBSTITUTION
            | TypeFlags::INDEX
            | TypeFlags::TEMPLATE_LITERAL
            | TypeFlags::STRING_MAPPING,
    )
}

#[derive(Default)]
struct Report {
    /// §A. `(has a structured constituent, matches the baseline today) -> lines`.
    cross: BTreeMap<(bool, bool), usize>,
    /// §A. Cases behind each cell, so concentration is beside every rate.
    cross_cases: BTreeMap<(bool, bool), HashMap<String, usize>>,
    /// The assignable-proxy verdict: would a constituent be dropped?
    would_change: BTreeMap<(bool, bool), usize>,
    /// §B. Among lines with a structured constituent, does the baseline keep every constituent
    /// we have? Keyed by whether we match today.
    baseline_keeps_all: BTreeMap<(bool, bool), usize>,
    /// The exact `ours -> baseline` pairs for structured lines we get wrong.
    pairs: BTreeMap<(String, String), usize>,
    /// Why a structured line we get wrong is wrong — the question §B's
    /// no-op/collapsed split cannot answer, because "collapsed" lumps subtype
    /// reduction together with everything else that shortens a union.
    families: BTreeMap<&'static str, usize>,
    /// For the nullable-not-stripped family (`bd tsr-e10`): the node kind of the
    /// line, and its parent's. The issue registers that the provenance must be
    /// established before the build is sized — a line reached through a path
    /// this port does not walk is not converted by adding a facts bit.
    nullable_sites: BTreeMap<(String, String), usize>,
    /// §C. Gap populations, by the guard that produced them. Populations only.
    gaps: BTreeMap<&'static str, usize>,
    gap_cases: BTreeMap<&'static str, HashMap<String, usize>>,
    unions_total: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        for (k, n) in &other.cross {
            *self.cross.entry(*k).or_default() += n;
        }
        for (k, cases) in &other.cross_cases {
            let mine = self.cross_cases.entry(*k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        for (k, n) in &other.would_change {
            *self.would_change.entry(*k).or_default() += n;
        }
        for (k, n) in &other.baseline_keeps_all {
            *self.baseline_keeps_all.entry(*k).or_default() += n;
        }
        for (k, n) in &other.pairs {
            *self.pairs.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.families {
            *self.families.entry(k).or_default() += n;
        }
        for (k, n) in &other.nullable_sites {
            *self.nullable_sites.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.gaps {
            *self.gaps.entry(k).or_default() += n;
        }
        for (k, cases) in &other.gap_cases {
            let mine = self.gap_cases.entry(k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        self.unions_total += other.unions_total;
    }
}

/// Why a union line is wrong, over the pair `(ours, the baseline)`.
///
/// Every arm is a **positive** test on the two strings and the arms are tried
/// in order, so a line lands in exactly one. The point of the partition is that
/// `removeSubtypes` can only account for the `collapse` arm: the other four
/// shorten, reorder or re-bracket a union for reasons that have nothing to do
/// with the subtype relation, and counting them as this item's population is
/// the "a number can be true and answer a different question" error.
fn family(ours: &str, want: &str) -> &'static str {
    let split = |s: &str| {
        let mut v: Vec<String> =
            s.split(" | ").map(|p| p.trim_matches(['(', ')']).to_owned()).collect();
        v.sort();
        v
    };
    let strip_parens = |s: &str| s.replace(['(', ')'], "");
    if strip_parens(ours) == strip_parens(want) {
        // `A & B | C & D` against `(A & B) | (C & D)`, and
        // `() => boolean | undefined` against `(() => boolean) | undefined`.
        return "printer: parenthesisation";
    }
    if split(ours) == split(want) {
        // Same constituents, different order — the `bd tsr-iiu` family.
        return "printer: constituent order";
    }
    for suffix in [" | undefined", " | null", " | undefined | null", " | null | undefined"] {
        if ours.strip_suffix(suffix) == Some(want) || ours.strip_prefix(&suffix[3..]).is_some() {
            return "narrowing: a nullable was not stripped";
        }
    }
    let (ours_parts, want_parts) = (split(ours), split(want));
    if want_parts.iter().all(|p| ours_parts.contains(p)) && want_parts.len() < ours_parts.len() {
        // The baseline is a strict subset of what we built. This is the only
        // arm `removeSubtypes` can explain — and even here it is a candidate,
        // not a conversion, because the relation must actually hold.
        return "a strict subset survives — removeSubtypes CANDIDATE";
    }
    if !ours.contains(" | ") || !want.contains(" | ") {
        return "not a union on one side — a different answer entirely";
    }
    "neither: the constituents themselves differ"
}

/// Which withheld guard a gap line sits behind, for §C. Syntactic only.
fn gap_guard(node: Option<Node<'_>>) -> Option<&'static str> {
    match node? {
        Node::BinaryExpression(binary) => match binary.operator_token?.kind {
            SyntaxKind::BarBarToken | SyntaxKind::BarBarEqualsToken => Some("`||`"),
            SyntaxKind::QuestionQuestionToken | SyntaxKind::QuestionQuestionEqualsToken => {
                Some("`??`")
            }
            _ => None,
        },
        Node::ConditionalExpression(_) => Some("`?:` branch union"),
        Node::ArrayLiteralExpression(_) => Some("array literal, object-reduction guard"),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<Report> {
    if case.has_varied_types() || case.has_known_divergence() {
        return None;
    }
    let text = case.expected_types()?;
    let expected = types_baseline::parse(&text);
    if types_baseline::assertion_count(&expected) == 0 {
        return None;
    }
    let parsed = case.load().ok()?;

    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));

    let mut report = Report::default();

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            let matches_today = want.text == got.line();
            let want_type = if matches_today {
                got.type_string.clone()
            } else {
                match want.text.strip_prefix(&format!("{} : ", got.text)) {
                    Some(rest) => rest.to_owned(),
                    // Unaligned: the walker got the expression text wrong, so no
                    // checker work touches this line. Not this probe's
                    // population, exactly as in `gaproot`.
                    None => continue,
                }
            };

            let id = line_ids[position];

            // §C. Gap lines behind a withheld reduction guard. Populations only:
            // their answers cannot be computed without the build.
            if got.type_string == "error" {
                if let Some(guard) = gap_guard(map.get(id)) {
                    *report.gaps.entry(guard).or_default() += 1;
                    *report
                        .gap_cases
                        .entry(guard)
                        .or_default()
                        .entry(case.name.clone())
                        .or_default() += 1;
                }
                continue;
            }

            // §A and §B: lines this port already answers as a union.
            let computed = types_producer::type_id_at_location(&mut checker, bound, nodes, map, id);
            let constituents = match &checker.type_of(computed).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => continue,
            };
            report.unions_total += 1;

            let structured: Vec<tsr_checker::TypeId> = constituents
                .iter()
                .copied()
                .filter(|&c| is_structured(checker.type_of(c).flags))
                .collect();
            let reducible = !structured.is_empty();
            let key = (reducible, matches_today);
            *report.cross.entry(key).or_default() += 1;
            *report.cross_cases.entry(key).or_default().entry(case.name.clone()).or_default() += 1;

            // The assignable-proxy verdict. Upstream removes on
            // `strictSubtypeRelation`; `Assignable` relates strictly more pairs,
            // so a `true` here is an upper bound on a real removal.
            let mut dropped = false;
            for &source in &structured {
                for &target in &constituents {
                    if source != target && checker.is_type_assignable_to(source, target) {
                        dropped = true;
                        break;
                    }
                }
                if dropped {
                    break;
                }
            }
            if dropped {
                *report.would_change.entry(key).or_default() += 1;
            }

            if reducible {
                // §B. Does the baseline keep every constituent we built? If so,
                // reduction is a no-op for this line and the relation is not
                // what the line is waiting on.
                let keeps_all = constituents.iter().all(|&c| {
                    let rendered = checker.type_to_string(c);
                    want_type.split(" | ").any(|part| part.trim_matches(['(', ')']) == rendered)
                });
                *report.baseline_keeps_all.entry((keeps_all, matches_today)).or_default() += 1;
                if !matches_today {
                    *report
                        .pairs
                        .entry((got.type_string.clone(), want_type.clone()))
                        .or_default() += 1;
                    let fam = family(&got.type_string, &want_type);
                    *report.families.entry(fam).or_default() += 1;
                    if fam == "narrowing: a nullable was not stripped" {
                        let kind = format!("{:?}", nodes.kind(id));
                        let parent = nodes.parent(id).map_or_else(
                            || "<root>".to_owned(),
                            |p| format!("{:?}", nodes.kind(p)),
                        );
                        *report.nullable_sites.entry((parent, kind)).or_default() += 1;
                    }
                }
            }
        }
    }
    Some(report)
}

fn top1(cases: Option<&HashMap<String, usize>>, total: usize) -> String {
    let Some(cases) = cases else { return String::new() };
    let (name, n) =
        cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
    #[allow(clippy::cast_precision_loss)]
    let share = *n as f64 / total.max(1) as f64 * 100.0;
    format!("{:>5} cases, top-1 {share:>5.1}%  {name}", cases.len())
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("discovering cases");

    let report = cases
        .par_iter()
        .filter_map(measure)
        .fold(Report::default, |mut acc, r| {
            acc.merge(&r);
            acc
        })
        .reduce(Report::default, |mut a, b| {
            a.merge(&b);
            a
        });

    println!("# subtypes — how much `removeSubtypes` would move (`bd tsr-eak`)\n");
    println!(
        "Upstream only considers a constituent for removal when it is StructuredOrInstantiable\n\
         (`checker.go:25955`), so `UnionReductionLiteral` and `UnionReductionSubtype` can only\n\
         disagree on a union with >= 2 structured constituents. That is the whole population.\n"
    );

    println!("## A. Lines this port already answers as a union: {}\n", report.unions_total);
    println!("    has structured   matches today        lines   would change (assignable-proxy)");
    for reducible in [true, false] {
        for matches in [true, false] {
            let key = (reducible, matches);
            let lines = report.cross.get(&key).copied().unwrap_or(0);
            let changed = report.would_change.get(&key).copied().unwrap_or(0);
            println!(
                "    {:<16} {:<18} {lines:>6}   {changed:>6}    {}",
                if reducible { "yes" } else { "no" },
                if matches { "RIGHT today" } else { "wrong today" },
                top1(report.cross_cases.get(&key), lines),
            );
        }
    }

    let c1 = report.would_change.get(&(false, true)).copied().unwrap_or(0)
        + report.would_change.get(&(false, false)).copied().unwrap_or(0);
    let c2: usize = report.cross.values().sum();
    println!(
        "\n  C1 (construction, upstream's gate at :25955): would-change with NO structured\n     constituent = {c1}  (expect 0; read 61 against the wrong \"fewer than two\" partition)"
    );
    println!(
        "  C2 (arithmetic): the four cells sum to {c2}, the union population is {}",
        report.unions_total
    );

    let at_risk = report.cross.get(&(true, true)).copied().unwrap_or(0);
    let could_gain = report.cross.get(&(true, false)).copied().unwrap_or(0);
    println!(
        "\n  **The number that decides this item**: {at_risk} lines are RIGHT today and carry a\n  structured constituent — every one is a line `removeSubtypes` can break. Against\n  {could_gain} wrong ones it could help."
    );

    println!("\n## B. Among the lines with a structured constituent, is reduction even a no-op?\n");
    println!("    baseline keeps every constituent we built   matches today     lines");
    for keeps in [true, false] {
        for matches in [true, false] {
            let n = report.baseline_keeps_all.get(&(keeps, matches)).copied().unwrap_or(0);
            println!(
                "    {:<43} {:<17} {n:>6}",
                if keeps {
                    "yes — reduction is a NO-OP"
                } else {
                    "no  — something was collapsed"
                },
                if matches { "RIGHT today" } else { "wrong today" },
            );
        }
    }

    println!("\n  WHY those lines are wrong. Only one arm is this item's population:\n");
    let mut families: Vec<_> = report.families.iter().collect();
    families.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let family_total: usize = report.families.values().sum();
    for (family, n) in &families {
        #[allow(clippy::cast_precision_loss)]
        let share = **n as f64 / family_total.max(1) as f64 * 100.0;
        println!("    {family:<48} {n:>6}  {share:>5.1}%");
    }
    println!("    {:<48} {family_total:>6}", "total");

    println!("\n  the exact `ours -> baseline` pairs, for the structured lines we get wrong:\n");
    let mut pairs: Vec<_> = report.pairs.iter().collect();
    pairs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((ours, want), n) in pairs.iter().take(12) {
        println!("      {n:>5}  {ours}\n             -> {want}");
    }

    println!("\n## B2. `bd tsr-e10` — where the nullable-not-stripped lines sit\n");
    println!("    parent kind                          node kind                       lines");
    let mut sites: Vec<_> = report.nullable_sites.iter().collect();
    sites.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((parent, kind), n) in sites.iter().take(14) {
        println!("    {parent:<36} {kind:<30} {n:>6}");
    }

    println!("\n## C. The gap rows — POPULATIONS, with no conversion beside them\n");
    println!("  Their answers cannot be computed without the build. A population is a ceiling.\n");
    let mut gaps: Vec<_> = report.gaps.iter().collect();
    gaps.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (guard, n) in gaps {
        println!("    {guard:<40} {n:>6}   {}", top1(report.gap_cases.get(guard), *n));
    }
}
