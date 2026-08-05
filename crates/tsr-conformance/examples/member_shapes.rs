//! What the receiver of a `has no such property` gap actually *is*, and whether
//! the two rows that carry it are one node counted twice.
//!
//! `cargo run -p tsr-conformance --example member_shapes --release`
//!
//! # The two questions, and why neither is answerable from the board
//!
//! `docs/architecture/checker-notes-rank.md` §3 ranks two rows beside each
//! other:
//!
//! | # | row | lines | cases |
//! |---|---|---:|---:|
//! | 5 | `member name, the receiver has no such property` | 4,016 | 1,060 |
//! | 6 | `property access, the receiver has no such property` | 4,005 | 1,053 |
//!
//! Both are emitted by one function — `types_producer::access_reason` — and the
//! board cannot say either of the two things a decision needs:
//!
//! 1. **Are they the same nodes?** Near-identical aggregates are the weakest
//!    possible evidence of pairing: two independent populations of the same size
//!    look exactly like one population counted twice. This probe keys every such
//!    line by the `PropertyAccessExpression` it came from and reports how many
//!    nodes contributed **both** lines, so the sum 4,016 + 4,005 is either
//!    earned or refused.
//! 2. **Whose failure is it?** `get_property_of_type`
//!    (`crates/tsr-checker/src/members.rs`) answers only for
//!    `TypeData::Named { members: Some(..) }` and `TypeData::Anonymous`, and
//!    returns `None` on its `_` arm for every other receiver shape **before it
//!    reads the name**. So "the receiver has no such property" is two opposite
//!    findings under one string: a lookup that ran and said no — which is
//!    upstream's answer too — and a lookup that never ran, which is ours.
//!    `docs/architecture/checker-notes-calls.md` measured exactly this split on
//!    the *call* path and found 89% of it was the second kind. This asks the
//!    same question one level over, on assertion lines rather than callee nodes.
//!
//! # The cheap instrument was tried first, and it is printed beside the real one
//!
//! `access_reason` puts the receiver's **printed** type in the reason string, so
//! a bucket over the uncut string classifies the receiver for free — no checker
//! change, no counters. That column is printed here as `by printed text`. Its
//! limit is exact and is why it is not the whole probe: a printed name cannot
//! distinguish `Named { members: Some }` from `Named { members: None }`, and on
//! the call path that distinction was 1,011 of 1,985. So the real column reads
//! `TypeData` off the receiver's `TypeId` through `Checker::type_of`, which is
//! public — **no `members.rs` change was needed to measure this**, and no
//! process-wide counters, which is also why this probe can run under `rayon`
//! while `overload_funnel` cannot.
//!
//! Where the two columns disagree is itself reported, because a disagreement is
//! the measurement of what the cheap cut cannot see.
//!
//! # Unit: assertion LINES
//!
//! Every number here is a `.types` **assertion line**, the same unit as
//! `rank_board`. `checker-notes-calls.md`'s 1,985/216/474/1,011 are call
//! expression **nodes** and are not comparable term for term. The board has
//! twice turned a node count into a line count by omission.
//!
//! # Control buckets
//!
//! Five, printed unconditionally, and every one must read zero. They are the
//! only evidence these buckets partition rather than merely being true:
//!
//! - `rows 5/6 lines not classified into a receiver shape`
//! - `reason says no such property but get_property_of_type finds one`
//! - `the line's node is not a property access or its name child`
//! - `pairing: lines - (2 x paired + name only + access only)`
//! - `shape roll-up - rows 5/6 total`
//!
//! # Denominator
//!
//! The suite's own skips, skip for skip, and the same alignment test
//! `rank_board` applies, so the row totals here are directly comparable with the
//! board's 4,016 and 4,005. **Reconciling them is a cross-check against a
//! different instrument**, which `docs/conventions.md` makes the difference
//! between a count and a prediction.

use std::collections::HashMap;

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_checker::TypeData;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Which of the two rows a line belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Row {
    /// Row 5: the assertion for the `b` of `a.b`, a leaf.
    MemberName,
    /// Row 6: the assertion for the access `a.b` itself.
    Access,
}

/// The receiver's `TypeData` variant — the question
/// `get_property_of_type` actually asks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Shape {
    /// `Named { members: Some(..) }`. The lookup ran over a real member table
    /// and the name was not in it. **Upstream reports "property does not exist"
    /// here too** — not ours.
    LookedUp,
    /// `Anonymous`. The lookup ran over the symbol's `exports` and missed.
    /// Mixed: upstream's own answer for a genuine miss, ours where the exports
    /// table is one this port never filled.
    Anonymous,
    /// An intrinsic receiver — `string`, `number`, `boolean`. Upstream reaches
    /// the wrapper interface through `getApparentType` (`checker.go:21729`),
    /// which is not ported. Ours.
    Intrinsic,
    /// A union or an intersection. Upstream distributes the lookup. Ours.
    Composite,
    /// `Named { members: None }` — a type this port built without a member
    /// table, which `create_type_reference` does for every instantiated generic
    /// (`bd tsr-4qx`, PARKED). Ours, and pointing at a parked item.
    NamedNoMembers,
    /// A literal type — `"abc".length`, `(1).toFixed()`. Upstream reaches these
    /// through the *same* `getApparentType` as an intrinsic
    /// (`checker.go:21729` maps a literal to its base primitive's wrapper), so
    /// this is a sibling of [`Shape::Intrinsic`] and not a leftover. It is a
    /// separate bucket rather than folded in because the two are different
    /// arms to write, and folding them would make the apparent-type item's size
    /// unfalsifiable.
    Literal,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Self::LookedUp => "receiver has members, name absent (upstream errors too)",
            Self::Anonymous => "receiver is Anonymous, name absent in exports",
            Self::Intrinsic => "receiver is an intrinsic (getApparentType, unported)",
            Self::Composite => "receiver is a union or intersection",
            Self::NamedNoMembers => "receiver is Named without members (bd tsr-4qx, PARKED)",
            Self::Literal => "receiver is a literal type (getApparentType, unported)",
        }
    }

    /// Every shape, in the order they are printed. There is deliberately **no
    /// `other` bucket**: the match that produces a `Shape` is exhaustive over
    /// `TypeData`, so a new type variant is a compile error rather than a
    /// silent row of leftovers. That is the stronger form of the same control —
    /// a runtime bucket that can only ever read zero is decoration.
    fn all() -> [Self; 6] {
        [
            Self::LookedUp,
            Self::Anonymous,
            Self::Intrinsic,
            Self::Composite,
            Self::NamedNoMembers,
            Self::Literal,
        ]
    }

    /// The bucket the cheap text cut would put this shape in, if the two agreed.
    /// Printed disagreements are the measurement of what the text cannot see.
    fn expected_text(self) -> TextShape {
        match self {
            Self::Intrinsic => TextShape::Intrinsic,
            Self::Composite => TextShape::Composite,
            Self::Literal => TextShape::Literal,
            _ => TextShape::Named,
        }
    }
}

/// The same question asked of the receiver's **printed** type, which is already
/// in the reason string and costs nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum TextShape {
    /// One of the printed intrinsic names.
    Intrinsic,
    /// Contains ` | ` or ` & ` at the top level of the printed form.
    Composite,
    /// A literal's printed form.
    Literal,
    /// Anything else — a name, a `typeof X`, a signature. **This is the bucket
    /// the cheap cut cannot split**, and the whole reason the `TypeData` column
    /// exists.
    Named,
}

impl TextShape {
    fn label(self) -> &'static str {
        match self {
            Self::Intrinsic => "printed as an intrinsic name",
            Self::Composite => "printed with | or &",
            Self::Literal => "printed as a literal",
            Self::Named => "printed as something else (NOT SPLITTABLE by text)",
        }
    }

    fn all() -> [Self; 4] {
        [Self::Intrinsic, Self::Composite, Self::Literal, Self::Named]
    }
}

/// Every name `type_to_string` prints for an intrinsic type.
///
/// Kept as a list rather than derived, because this classifier deliberately
/// works on the *string* — it is the cheap instrument, and giving it access to
/// the type would make it the same instrument as the other column.
const INTRINSIC_NAMES: [&str; 14] = [
    "any",
    "unknown",
    "string",
    "number",
    "bigint",
    "boolean",
    "symbol",
    "void",
    "undefined",
    "null",
    "never",
    "object",
    "error",
    "true",
];

fn text_shape(printed: &str) -> TextShape {
    if INTRINSIC_NAMES.contains(&printed) {
        return TextShape::Intrinsic;
    }
    if printed.contains(" | ") || printed.contains(" & ") {
        return TextShape::Composite;
    }
    if printed.starts_with('"')
        || printed.starts_with('\'')
        || printed.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        return TextShape::Literal;
    }
    TextShape::Named
}

/// The receiver's printed type, as `access_reason` appended it.
fn printed_receiver(reason: &str) -> Option<&str> {
    let cut = "has no such property: ";
    reason.find(cut).map(|at| &reason[at + cut.len()..])
}

/// Which row a `has no such property` reason belongs to, or `None` if the
/// reason is not one of the two rows at all.
fn row_of(reason: &str) -> Option<Row> {
    if !reason.contains("the receiver has no such property") {
        return None;
    }
    if reason.starts_with("member name,") {
        Some(Row::MemberName)
    } else if reason.starts_with("property access,") {
        Some(Row::Access)
    } else {
        None
    }
}

/// The two properties this probe rests on, checked **before** anything is
/// measured and on every run.
///
/// Assertions in `main` rather than `#[cfg(test)]`, for the reason
/// `rank_board`'s `check_classifier` gives: Cargo does not run tests inside an
/// example without a manifest change, and `crates/tsr-conformance/Cargo.toml` is
/// shared. A check that runs on every measurement is the stronger statement in
/// any case.
fn check_classifier() {
    // ORDER / MEMBERSHIP. `... the receiver is a gap: Identifier` is the other
    // half of the same function and is rows 3/4, not 5/6. Admitting it would
    // add 13,500 lines of a different work item to this probe's population.
    assert!(
        row_of("property access, the receiver is a gap: Identifier").is_none(),
        "a gapped receiver is a different row and must not enter this population"
    );
    assert_eq!(
        row_of("member name, the receiver has no such property: Promise<boolean>"),
        Some(Row::MemberName),
        "the member-name row is the one whose reason starts with `member name,`"
    );
    assert_eq!(
        row_of("property access, the receiver has no such property: SymbolConstructor"),
        Some(Row::Access),
        "the access row is the one whose reason starts with `property access,`"
    );

    // THE CHEAP CUT'S LIMIT, asserted rather than described. `Promise<number>`
    // prints identically whether or not the type carries a member table, so the
    // text column cannot be the answer to the question this probe asks. If this
    // assertion ever fails, the text column has become sufficient and the
    // `TypeData` column can be dropped.
    assert_eq!(
        text_shape("Promise<number>"),
        TextShape::Named,
        "a printed name cannot say whether the type has a member table"
    );
    assert_eq!(
        text_shape("number"),
        TextShape::Intrinsic,
        "an intrinsic receiver is the one shape the cheap cut does decide"
    );
    assert_eq!(text_shape("A | B"), TextShape::Composite, "a union prints with a bar");
    assert_eq!(
        printed_receiver("member name, the receiver has no such property: A | B"),
        Some("A | B"),
        "the receiver's printed type is the tail of the reason"
    );
}

/// What happened to the *other* line of an access that contributed only one.
///
/// The two rows can only be one work item if the lines pair up, and the
/// interesting number is not the pairing rate but the **residual**: 4,016
/// against 4,005 is eleven lines that must be explained by something, and
/// "they look paired" is not an explanation. Each variant is a different
/// reason a pair can be broken, and only [`Unpaired::OtherGap`] would mean the
/// two rows are genuinely separate work at that site.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Unpaired {
    /// The access has no name child with a node id — `a.#b`, or a name form
    /// `access_reason` routes to a different string.
    NoNode,
    /// The node exists and the walker emitted no assertion for it, or upstream
    /// has no line at that position.
    NotEmitted,
    /// The line exists and our expression text does not match upstream's, so it
    /// never entered the aligned population either row is counted over.
    Unaligned,
    /// The other line is **right**. The two rows are not one item at this site.
    Matched,
    /// The other line is wrong with a real type — ported and defective.
    Wrong,
    /// The other line gapped for a *different* reason. The only variant that
    /// makes the two rows separate work rather than one node counted twice.
    OtherGap,
}

impl Unpaired {
    fn label(self) -> &'static str {
        match self {
            Self::NoNode => "the access has no identifier name child",
            Self::NotEmitted => "no assertion emitted at the other position",
            Self::Unaligned => "the other line is unaligned (outside both rows)",
            Self::Matched => "the other line is RIGHT",
            Self::Wrong => "the other line is wrong with a real type",
            Self::OtherGap => "the other line gapped for a DIFFERENT reason",
        }
    }

    fn all() -> [Self; 6] {
        [
            Self::NoNode,
            Self::NotEmitted,
            Self::Unaligned,
            Self::Matched,
            Self::Wrong,
            Self::OtherGap,
        ]
    }
}

/// One case's contribution.
struct CaseReport {
    name: String,
    /// Upstream lines this case has.
    expected: usize,
    /// Lines we reproduced exactly — text *and* type.
    matched: usize,
    /// Rows 5/6 lines by `(row, shape)`.
    lines: HashMap<(Row, Shape), usize>,
    /// The same lines by the cheap column.
    text_lines: HashMap<TextShape, usize>,
    /// Where the two columns disagree, as `printed-text bucket -> TypeData
    /// bucket`, so a disagreement names itself.
    disagreements: HashMap<(TextShape, Shape), usize>,
    /// Rows 5/6 lines by shape, for the concentration and level-4 passes.
    by_shape: HashMap<Shape, usize>,
    /// Access nodes that contributed **both** lines.
    paired: usize,
    /// Access nodes that contributed only the member-name line.
    name_only: usize,
    /// Access nodes that contributed only the access line.
    access_only: usize,
    /// Why each unpaired access is unpaired.
    unpaired: HashMap<Unpaired, usize>,
    /// The property name that was not found, by shape — bounded per case.
    names: HashMap<(Shape, String), usize>,
    /// The receiver's printed type for the intrinsic bucket, so the
    /// `getApparentType` claim names which primitives.
    intrinsics: HashMap<String, usize>,
    /// CONTROL: a rows-5/6 line whose node is not a property access or the name
    /// child of one.
    control_not_access: usize,
    /// CONTROL: the reason says no such property and the lookup finds one.
    control_lookup_found: usize,
    /// CONTROL: a rows-5/6 line that reached no shape bucket at all.
    control_unclassified: usize,
}

fn main() {
    check_classifier();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let mut cases = corpus.discover().expect("discovering cases");
    // A smoke cap, so the probe can be exercised for panics and for its control
    // buckets without spending a full corpus pass. **A capped run is not a
    // measurement** — every share it prints is a share of a subset — so the
    // number of cases judged is printed on every run and the caller is expected
    // to reconcile it against the board's 9,538.
    if let Ok(limit) = std::env::var("TSR_MEMBER_SHAPES_LIMIT")
        && let Ok(limit) = limit.parse::<usize>()
    {
        cases.truncate(limit);
    }

    let reports: Vec<CaseReport> = cases
        .par_iter()
        .filter_map(|case| {
            // The suite's own skips, skip for skip, so every share here is a
            // share of the gradient's denominator.
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
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
            let nodes = program.nodes();
            let node_map = program.node_map();
            let bound = program.binder();
            let mut checker = tsr_checker::Checker::new(bound, nodes, node_map);

            let mut report = CaseReport {
                name: case.name.clone(),
                expected: 0,
                matched: 0,
                lines: HashMap::new(),
                text_lines: HashMap::new(),
                disagreements: HashMap::new(),
                by_shape: HashMap::new(),
                paired: 0,
                name_only: 0,
                access_only: 0,
                unpaired: HashMap::new(),
                names: HashMap::new(),
                intrinsics: HashMap::new(),
                control_not_access: 0,
                control_lookup_found: 0,
                control_unclassified: 0,
            };

            for (index, expected_file) in expected.iter().enumerate() {
                let our_file = ours.get(index);
                let our_ids = ids.get(index);
                // The pairing tally, per file: how many of each row every
                // access node contributed. Per file rather than per case,
                // because ids are per parse and two units are two parses.
                let mut per_access: HashMap<NodeId, (usize, usize)> = HashMap::new();
                // Where each node's assertion sits in this file's rendering, so
                // an *unpaired* access can be asked what happened to its other
                // line rather than being reported as a bare residual. First
                // wins, matching the walker's own preorder.
                let mut position_of: HashMap<NodeId, usize> = HashMap::new();
                if let Some(line_ids) = our_ids {
                    for (position, id) in line_ids.iter().enumerate() {
                        position_of.entry(*id).or_insert(position);
                    }
                }

                for (position, want) in expected_file.assertions.iter().enumerate() {
                    report.expected += 1;
                    let Some(got) = our_file.and_then(|file| file.get(position)) else { continue };
                    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    if want_type == got.type_string {
                        report.matched += 1;
                        continue;
                    }
                    if got.type_string != "error" {
                        continue;
                    }
                    let id = our_ids.expect("ids beside a rendered file")[position];
                    let reason =
                        types_producer::gap_reason(&mut checker, bound, nodes, node_map, id);
                    let Some(row) = row_of(&reason) else {
                        // CONTROL. A reason that says "has no such property"
                        // and starts with neither prefix belongs to these rows
                        // and reached no bucket. Zero unless `gap_reason` grows
                        // a third caller of `access_reason`.
                        if reason.contains("the receiver has no such property") {
                            report.control_unclassified += 1;
                        }
                        continue;
                    };

                    // The access node this line belongs to. Row 6 *is* the
                    // access; row 5 is its name child, so one hop up.
                    let access_id = match row {
                        Row::Access => Some(id),
                        Row::MemberName => nodes.parent(id),
                    }
                    .filter(|&candidate| {
                        nodes.kind(candidate) == SyntaxKind::PropertyAccessExpression
                    });
                    let Some(access_id) = access_id else {
                        report.control_not_access += 1;
                        continue;
                    };
                    let Some(Node::PropertyAccessExpression(access)) = node_map.get(access_id)
                    else {
                        report.control_not_access += 1;
                        continue;
                    };
                    let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
                        (access.expression, access.name)
                    else {
                        report.control_not_access += 1;
                        continue;
                    };

                    let entry = per_access.entry(access_id).or_default();
                    match row {
                        Row::MemberName => entry.0 += 1,
                        Row::Access => entry.1 += 1,
                    }

                    // The receiver's type, from the same checker the reason was
                    // computed with, so this is the type `get_property_of_type`
                    // was handed and not a re-derivation.
                    let receiver_type = checker.check_expression(receiver);
                    if checker.get_property_of_type(receiver_type, name.text).is_some() {
                        // `access_reason` reaches this row only when the lookup
                        // answered `None`. A hit here means the two have drifted
                        // apart and nothing below it is a partition.
                        report.control_lookup_found += 1;
                    }
                    let printed = checker.type_to_string(receiver_type);
                    let shape = match &checker.type_of(receiver_type).data {
                        TypeData::Named { members: Some(_), .. } => Shape::LookedUp,
                        TypeData::Named { members: None, .. } => Shape::NamedNoMembers,
                        TypeData::Anonymous { .. } => Shape::Anonymous,
                        TypeData::Intrinsic { .. } => Shape::Intrinsic,
                        TypeData::Union { .. } | TypeData::Intersection { .. } => Shape::Composite,
                        TypeData::StringLiteral(_)
                        | TypeData::NumberLiteral(_)
                        | TypeData::BigIntLiteral(_)
                        | TypeData::BooleanLiteral(_) => Shape::Literal,
                    };
                    let text_bucket = text_shape(&printed);

                    *report.lines.entry((row, shape)).or_default() += 1;
                    *report.by_shape.entry(shape).or_default() += 1;
                    *report.text_lines.entry(text_bucket).or_default() += 1;
                    if text_bucket != shape.expected_text() {
                        *report.disagreements.entry((text_bucket, shape)).or_default() += 1;
                    }
                    if report.names.len() < 4_000 {
                        *report.names.entry((shape, name.text.to_string())).or_default() += 1;
                    }
                    if matches!(shape, Shape::Intrinsic | Shape::Literal) {
                        *report.intrinsics.entry(printed).or_default() += 1;
                    }
                }

                for (access_id, (member_name_lines, access_lines)) in per_access {
                    let both = member_name_lines.min(access_lines);
                    report.paired += both;
                    report.name_only += member_name_lines - both;
                    report.access_only += access_lines - both;
                    if member_name_lines == access_lines {
                        continue;
                    }
                    // The side that is *missing*: if only the member name
                    // gapped here, the unexplained line is the access, and the
                    // other way round. `docs/conventions.md` — an unexplained
                    // residual is what turns "they look paired" into a story.
                    let missing = if member_name_lines > access_lines {
                        Some(access_id)
                    } else {
                        node_map.get(access_id).and_then(|node| node.name_id())
                    };
                    let outcome = missing.map_or(Unpaired::NoNode, |id| {
                        let Some(&position) = position_of.get(&id) else {
                            return Unpaired::NotEmitted;
                        };
                        let (Some(got), Some(want)) = (
                            our_file.and_then(|file| file.get(position)),
                            expected_file.assertions.get(position),
                        ) else {
                            return Unpaired::NotEmitted;
                        };
                        let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                        else {
                            return Unpaired::Unaligned;
                        };
                        if want_type == got.type_string {
                            Unpaired::Matched
                        } else if got.type_string == "error" {
                            Unpaired::OtherGap
                        } else {
                            Unpaired::Wrong
                        }
                    });
                    *report.unpaired.entry(outcome).or_default() += 1;
                }
            }
            Some(report)
        })
        .collect();

    report(&reports);
}

/// A share of a total, as a percentage.
#[allow(clippy::cast_precision_loss)]
fn pct(count: usize, total: usize) -> f64 {
    if total == 0 { 0.0 } else { 100.0 * (count as f64) / (total as f64) }
}

/// The signed difference of two counts, so a broken partition prints as a
/// negative number rather than as an enormous positive one.
fn delta(left: usize, right: usize) -> i128 {
    i128::try_from(left).expect("fits") - i128::try_from(right).expect("fits")
}

/// The concentration check `docs/conventions.md` makes the first command: how
/// many cases, and how much of the population sits in the largest of them.
fn concentration(label: &str, per_case: &HashMap<String, usize>) {
    let total: usize = per_case.values().sum();
    let mut rows: Vec<_> = per_case.iter().map(|(name, count)| (*count, name.clone())).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    let top1 = rows.first().map_or(0, |(count, _)| *count);
    let top10: usize = rows.iter().take(10).map(|(count, _)| count).sum();
    println!(
        "  {label}: {total} lines over {} cases, top-1 {:.1}%, top-10 {:.1}%",
        rows.len(),
        pct(top1, total),
        pct(top10, total)
    );
    for (count, name) in rows.iter().take(5) {
        println!("      {count:>6}  {:>5.1}%  {name}", pct(*count, total));
    }
}

/// The level-4 statistic (`docs/conventions.md`, "A fourth level"): over the
/// cases this population touches, how many would have **nothing left** if it
/// were closed. The `0` bucket *is* the question; everything else is context.
fn level_four(label: &str, per_case: &HashMap<String, usize>, residuals: &HashMap<String, usize>) {
    let mut buckets = [0usize; 6];
    let mut cases = 0usize;
    let mut remaining_total = 0usize;
    let mut all: Vec<usize> = Vec::new();
    for (name, count) in per_case {
        let residual = residuals.get(name).copied().unwrap_or_default();
        let remaining = residual.saturating_sub(*count);
        cases += 1;
        remaining_total += remaining;
        all.push(remaining);
        let bucket = match remaining {
            0 => 0,
            1..=5 => 1,
            6..=10 => 2,
            11..=25 => 3,
            26..=100 => 4,
            _ => 5,
        };
        buckets[bucket] += 1;
    }
    all.sort_unstable();
    let median = all.get(all.len() / 2).copied().unwrap_or_default();
    println!("  {label}: {cases} affected cases, median {median} other failing lines");
    for (bucket, name) in ["0", "1-5", "6-10", "11-25", "26-100", ">100"].into_iter().enumerate() {
        println!(
            "      remaining {name:>7}: {:>5} cases  {:>5.1}%",
            buckets[bucket],
            pct(buckets[bucket], cases)
        );
    }
    println!("      (total remaining failing lines in those cases: {remaining_total})");
}

#[allow(clippy::too_many_lines)]
fn report(reports: &[CaseReport]) {
    let mut lines: HashMap<(Row, Shape), usize> = HashMap::new();
    let mut text_lines: HashMap<TextShape, usize> = HashMap::new();
    let mut disagreements: HashMap<(TextShape, Shape), usize> = HashMap::new();
    let mut names: HashMap<(Shape, String), usize> = HashMap::new();
    let mut intrinsics: HashMap<String, usize> = HashMap::new();
    let mut by_shape_case: HashMap<Shape, HashMap<String, usize>> = HashMap::new();
    let mut rows_case: HashMap<String, usize> = HashMap::new();
    let mut residuals: HashMap<String, usize> = HashMap::new();
    let (mut paired, mut name_only, mut access_only) = (0usize, 0usize, 0usize);
    let mut unpaired: HashMap<Unpaired, usize> = HashMap::new();
    let (mut not_access, mut lookup_found, mut unclassified) = (0usize, 0usize, 0usize);
    let (mut expected, mut matched) = (0usize, 0usize);

    for case in reports {
        expected += case.expected;
        matched += case.matched;
        residuals.insert(case.name.clone(), case.expected - case.matched);
        for (key, count) in &case.lines {
            *lines.entry(*key).or_default() += count;
        }
        for (key, count) in &case.text_lines {
            *text_lines.entry(*key).or_default() += count;
        }
        for (key, count) in &case.disagreements {
            *disagreements.entry(*key).or_default() += count;
        }
        for (key, count) in &case.names {
            *names.entry(key.clone()).or_default() += count;
        }
        for (key, count) in &case.intrinsics {
            *intrinsics.entry(key.clone()).or_default() += count;
        }
        for (shape, count) in &case.by_shape {
            *by_shape_case.entry(*shape).or_default().entry(case.name.clone()).or_default() +=
                count;
            *rows_case.entry(case.name.clone()).or_default() += count;
        }
        for (key, count) in &case.unpaired {
            *unpaired.entry(*key).or_default() += count;
        }
        paired += case.paired;
        name_only += case.name_only;
        access_only += case.access_only;
        not_access += case.control_not_access;
        lookup_found += case.control_lookup_found;
        unclassified += case.control_unclassified;
    }

    let name_lines: usize =
        lines.iter().filter(|((row, _), _)| *row == Row::MemberName).map(|(_, c)| c).sum();
    let access_lines_total: usize =
        lines.iter().filter(|((row, _), _)| *row == Row::Access).map(|(_, c)| c).sum();
    let total = name_lines + access_lines_total;

    println!("cases judged             {}", reports.len());
    println!("assertion lines upstream {expected}");
    println!("exactly matched          {matched}  ({:.2}%)", pct(matched, expected));
    println!();
    println!("ROWS 5 and 6, as assertion lines");
    println!("  row 5  member name, the receiver has no such property   {name_lines:>7}");
    println!("  row 6  property access, the receiver has no such prop.  {access_lines_total:>7}");
    println!("  sum                                                    {total:>7}");
    println!("  (the board at 33e3bd5 read 4,016 and 4,005 — reconcile before quoting)");

    println!();
    println!("ARE THEY ONE NODE COUNTED TWICE? (property-access nodes)");
    println!(
        "  nodes contributing BOTH lines            {paired:>7}  ({:.1}% of nodes)",
        pct(paired, paired + name_only + access_only)
    );
    println!("  nodes contributing only the member name  {name_only:>7}");
    println!("  nodes contributing only the access       {access_only:>7}");
    println!(
        "  lines a one-fix claim may quote          {:>7}   = 2 x paired + name only + access only",
        paired * 2 + name_only + access_only
    );
    println!("  and the unpaired accesses, explained:");
    for outcome in Unpaired::all() {
        let count = unpaired.get(&outcome).copied().unwrap_or_default();
        println!("      {count:>6}  {}", outcome.label());
    }

    println!();
    println!("BY RECEIVER SHAPE (TypeData — the question get_property_of_type asks)");
    for shape in Shape::all() {
        let count: usize = by_shape_case.get(&shape).map(|m| m.values().sum()).unwrap_or_default();
        println!("  {count:>7}  {:>5.1}%  {}", pct(count, total), shape.label());
    }

    println!();
    println!("BY PRINTED TEXT (the cheap cut — free, and it cannot split the named bucket)");
    for bucket in TextShape::all() {
        let count = text_lines.get(&bucket).copied().unwrap_or_default();
        println!("  {count:>7}  {:>5.1}%  {}", pct(count, total), bucket.label());
    }
    println!("  where the two columns disagree:");
    let mut rows: Vec<_> =
        disagreements.iter().map(|((text, shape), count)| (*count, *text, *shape)).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, text, shape) in rows.iter().take(10) {
        println!("      {count:>6}  {} -> {}", text.label(), shape.label());
    }

    println!();
    println!("CONCENTRATION, per shape (the parent's shape is not the child's)");
    concentration("rows 5+6 together", &rows_case);
    for shape in Shape::all() {
        if let Some(per_case) = by_shape_case.get(&shape) {
            concentration(shape.label(), per_case);
        }
    }

    println!();
    println!("LEVEL 4 — what else fails in the cases each shape touches");
    level_four("rows 5+6 together", &rows_case, &residuals);
    for shape in Shape::all() {
        if let Some(per_case) = by_shape_case.get(&shape) {
            level_four(shape.label(), per_case, &residuals);
        }
    }

    println!();
    println!("THE NAME THAT WAS NOT FOUND, per shape (top 12 each)");
    for shape in Shape::all() {
        let mut rows: Vec<_> = names
            .iter()
            .filter(|((s, _), _)| *s == shape)
            .map(|((_, name), count)| (*count, name.clone()))
            .collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort_unstable_by(|a, b| b.cmp(a));
        println!("  [{}]", shape.label());
        for (count, name) in rows.iter().take(12) {
            println!("      {count:>6}  {name}");
        }
    }

    println!();
    println!("THE INTRINSIC RECEIVERS, by printed type (what getApparentType would answer for)");
    let mut rows: Vec<_> = intrinsics.iter().map(|(name, count)| (*count, name.clone())).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, name) in rows.iter().take(15) {
        println!("      {count:>6}  {name}");
    }

    println!();
    println!("CONTROL BUCKETS — every one must read zero");
    println!("  CONTROL rows 5/6 lines with no receiver shape        = {unclassified}");
    println!("  CONTROL node is not a property access or its name    = {not_access}");
    println!("  CONTROL reason says no property, lookup finds one    = {lookup_found}");
    let shape_total: usize = by_shape_case.values().map(|m| m.values().sum::<usize>()).sum();
    println!(
        "  CONTROL shape roll-up - rows 5/6 total               = {}",
        delta(shape_total, total)
    );
    println!(
        "  CONTROL pairing - rows 5/6 total                     = {}",
        delta(paired * 2 + name_only + access_only, total)
    );
}
