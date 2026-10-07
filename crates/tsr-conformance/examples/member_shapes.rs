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
//! Six, printed unconditionally, and every one must read zero. They are the
//! only evidence these buckets partition rather than merely being true:
//!
//! - `rows 5/6 lines not classified into a receiver shape`
//! - `reason says no such property but get_property_of_type finds one`
//! - `the line's node is not a property access or its name child`
//! - `pairing: lines - (2 x paired + name only + access only)`
//! - `shape roll-up - rows 5/6 total`
//! - **`row 5 (a LEAF) counted as propagated/span`** — row 5 is the `b` of
//!   `a.b`, so nothing can be nested strictly inside its span and this is
//!   impossible *by construction*. It exists because the span test was first
//!   copied from `rank_board` with its polarity inverted, producing an entirely
//!   plausible table; see `docs/architecture/checker-notes-members.md` §5.3.
//!   A control whose value is fixed by the structure of what is measured, rather
//!   than by the measurement, is the only kind that catches this class of bug.
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

    // THE DISTRIBUTION RULE. A union needs EVERY constituent to have the
    // property; an intersection needs any one. Swapping them would move the
    // whole composite bucket into "ours" without anything changing.
    assert_eq!(
        distributed_outcome(true, 1, 2),
        Distributed::Partial,
        "a union with one of two constituents holding the property is Partial"
    );
    assert_eq!(
        distributed_outcome(false, 1, 2),
        Distributed::WouldFind,
        "an intersection needs only one constituent to hold the property"
    );
    assert_eq!(
        distributed_outcome(true, 0, 2),
        Distributed::NoneHaveIt,
        "no constituent holding the property means the union is not the blocker"
    );

    // THE NARROWING TEST. `upstream A` against `ours A | undefined` is upstream
    // being NARROWER — narrowing, or a nullish constituent it removed — and not
    // a print-order difference. Collapsing the two would hide the one result
    // that decides whether the composite bucket belongs to flow.rs.
    assert_eq!(
        agreement("A | B", "B | A"),
        Agreement::SameSetDifferentOrder,
        "the same constituents in another order is a printing difference"
    );
    assert_eq!(
        agreement("A", "A | undefined"),
        Agreement::UpstreamNarrower,
        "upstream holding a strict subset of our constituents is narrowing"
    );
    assert_eq!(agreement("A", "B"), Agreement::Differs, "a different type is not narrowing");

    // THE RECONCILIATION'S OWN ASSUMPTION, asserted rather than assumed. Rows
    // 5/6 can only ever be TERMINAL or propagated/span under `rank_board`'s
    // `cause()`: their reason contains neither "the receiver is a gap" (that is
    // rows 3/4) nor "/ initialiser " nor "/ annotation ", which are the only
    // routes to the other two causes. If that stops being true, this probe's
    // two-variant `Cause` silently merges a third bucket into one of these and
    // the reconciliation is worthless.
    for reason in [
        "member name, the receiver has no such property: A | B",
        "property access, the receiver has no such property: Promise<number>",
    ] {
        assert!(
            !reason.contains("the receiver is a gap")
                && !reason.contains("/ initialiser ")
                && !reason.contains("/ annotation "),
            "rows 5/6 must reach only TERMINAL or propagated/span in rank_board's cause()"
        );
    }
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

/// `rank_board`'s TERMINAL / PROPAGATED split, reproduced here for exactly one
/// purpose: reconciling this probe's row totals against the board's.
///
/// The board reads 4,016 and **4,005**; this probe read 4,016 and **4,008** at
/// the same commit, with no checker change in between. Three lines is 0.07% and
/// waving it through is precisely what `docs/conventions.md` forbids — an
/// unexplained numerator gap means two instruments are not measuring the same
/// thing, which is how `writer_guards`'s rate was quoted as the gradient for
/// four documents.
///
/// The hypothesis this measures: **`rank_board` keys every gap row by
/// `(row_key, cause)` and prints its rankings per cause**, so the board's 4,005
/// is the row's *propagated/span* count and not its total. Row 5 is a leaf —
/// nothing can be nested inside it — so it is 100% TERMINAL and matches
/// exactly, which is consistent. If that is right, this probe will read
/// **4,005 propagated + 3 terminal** for row 6 and the instruments agree
/// exactly.
///
/// The span test is copied from `rank_board` rather than re-derived: a
/// reconciliation whose two sides were computed differently reconciles nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Cause {
    /// Nothing gapped strictly inside this node's span.
    Terminal,
    /// Something inside the span gapped — for row 6 that is normally its own
    /// name child, which is row 5.
    PropagatedSpan,
}

impl Cause {
    fn label(self) -> &'static str {
        match self {
            Self::Terminal => "TERMINAL",
            Self::PropagatedSpan => "propagated/span",
        }
    }
}

/// What a union or intersection receiver would answer **if the lookup were
/// distributed over its constituents** — the "probe past the blocker" that
/// `docs/conventions.md` requires for a dependency-gated form.
///
/// Probing the blocker itself only shows it is live. This asks the question one
/// step further on: strip `undefined` and `null` (upstream's
/// `checkNonNullExpression` runs before the lookup) and ask
/// `get_property_of_type` of each remaining constituent. Upstream's rule is
/// `getPropertyOfUnionOrIntersectionType`: a **union** answers only if *every*
/// constituent has the property, an **intersection** if *any* does.
///
/// This is what separates a members-lookup item from a narrowing item. If the
/// constituents have the property, a distribution arm in
/// `crates/tsr-checker/src/members.rs` finds it and the work is mine. If they
/// do not, the union is not the blocker and the owner is upstream of it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Distributed {
    /// Upstream's rule is satisfied by the constituents we already have: a
    /// distribution arm would find the property today.
    WouldFind,
    /// Some constituents have it and the rule is not satisfied. Upstream
    /// reports "property does not exist on some constituent" here too — mixed,
    /// and not straightforwardly ours.
    Partial,
    /// No constituent has the property. The union is **not** the blocker; the
    /// constituents' own member tables are, which chains into `bd tsr-4qx` or
    /// the apparent type.
    NoneHaveIt,
}

impl Distributed {
    fn label(self) -> &'static str {
        match self {
            Self::WouldFind => "a distribution arm WOULD find it (ours, members.rs)",
            Self::Partial => "some constituents have it (upstream errors too)",
            Self::NoneHaveIt => "NO constituent has it (the union is not the blocker)",
        }
    }

    fn all() -> [Self; 3] {
        [Self::WouldFind, Self::Partial, Self::NoneHaveIt]
    }
}

/// How upstream's own type for the receiver *position* relates to ours.
///
/// The lead's challenge, and it is the right one: `kind` and `type` on a union,
/// in files named `discriminantPropertyCheck` and `controlFlowAliasing`, is a
/// **narrowing** signature. If upstream never had a union there, the work is in
/// `crates/tsr-checker/src/flow.rs` and not in `members.rs`, and calling it a
/// union-lookup item would be `docs/conventions.md`'s "a row named after a
/// mechanism is usually not about that mechanism" for the fourth time.
///
/// The constituent sets are compared by splitting the printed form on `" | "`,
/// which is a **heuristic**: it mis-splits a nested function type such as
/// `(() => void) | ((a: number) => void)`. It is labelled one, and it is not
/// what the ownership decision rests on — [`Distributed`] is. Its job is to say
/// whether the raw `DIFFERS` count is narrowing or merely constituent order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Agreement {
    /// Identical printed form.
    Same,
    /// The same constituents in a different printed order — our union sort is
    /// not upstream's. A printing difference, not a typing one.
    SameSetDifferentOrder,
    /// Upstream's constituents are a strict subset of ours: upstream had a
    /// **narrower** type at that position. Narrowing, or a nullish constituent
    /// upstream removed. **Not a members.rs item.**
    UpstreamNarrower,
    /// Neither — an alias printed as its name, or a genuinely different type.
    Differs,
    /// The receiver has no aligned assertion to compare against.
    NoComparison,
}

impl Agreement {
    fn label(self) -> &'static str {
        match self {
            Self::Same => "upstream's receiver type is ours, exactly",
            Self::SameSetDifferentOrder => "same constituents, different print order (ours)",
            Self::UpstreamNarrower => "upstream is NARROWER (narrowing — flow.rs, not ours)",
            Self::Differs => "differs another way (alias printing, or a real difference)",
            Self::NoComparison => "no aligned receiver line to compare",
        }
    }

    fn all() -> [Self; 5] {
        [
            Self::Same,
            Self::SameSetDifferentOrder,
            Self::UpstreamNarrower,
            Self::Differs,
            Self::NoComparison,
        ]
    }
}

/// Upstream's rule, `getPropertyOfUnionOrIntersectionType`, as a function so it
/// can be asserted rather than read: a **union** answers only when *every*
/// remaining constituent has the property; an **intersection** when *any* does.
///
/// Getting this backwards is the single edit that would turn the composite
/// bucket from "not ours" into "ours" in the output while changing nothing
/// real, which is why it is a named mutation rather than an inline expression.
fn distributed_outcome(is_union: bool, found: usize, kept: usize) -> Distributed {
    if found == 0 {
        Distributed::NoneHaveIt
    } else if !is_union || found == kept {
        Distributed::WouldFind
    } else {
        Distributed::Partial
    }
}

/// Compare upstream's printed receiver type with ours. See [`Agreement`] for
/// why the constituent split is a heuristic.
fn agreement(theirs: &str, ours: &str) -> Agreement {
    if theirs == ours {
        return Agreement::Same;
    }
    let mut their_parts: Vec<&str> = theirs.split(" | ").map(str::trim).collect();
    let mut our_parts: Vec<&str> = ours.split(" | ").map(str::trim).collect();
    their_parts.sort_unstable();
    their_parts.dedup();
    our_parts.sort_unstable();
    our_parts.dedup();
    if their_parts == our_parts {
        return Agreement::SameSetDifferentOrder;
    }
    if their_parts.iter().all(|part| our_parts.contains(part)) {
        return Agreement::UpstreamNarrower;
    }
    Agreement::Differs
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
    /// Rows 5/6 lines by `(row, cause)` — the reconciliation against the board.
    causes: HashMap<(Row, Cause), usize>,
    /// The row-6 lines whose cause is TERMINAL, named rather than counted:
    /// `(case, the assertion text)`. Bounded, because three lines need three
    /// examples and a runaway list would mean the hypothesis is wrong anyway.
    terminal_examples: Vec<(String, String)>,
    /// What a distributed lookup would answer, for composite receivers only.
    distributed: HashMap<Distributed, usize>,
    /// Composite-receiver lines whose access is an optional chain (`a?.b`),
    /// and lines whose union carried `undefined` or `null` at all.
    optional_chain: usize,
    nullish_in_union: usize,
    /// Upstream's own type for the receiver's position, against ours, for
    /// composite receivers: `(upstream, ours)`, bounded per case.
    receiver_pairs: HashMap<(String, String), usize>,
    /// The same comparison, classified rather than listed.
    agreements: HashMap<Agreement, usize>,
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
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
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
                causes: HashMap::new(),
                terminal_examples: Vec::new(),
                distributed: HashMap::new(),
                optional_chain: 0,
                nullish_in_union: 0,
                receiver_pairs: HashMap::new(),
                agreements: HashMap::new(),
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

                // `rank_board`'s span test, copied verbatim so the two sides of
                // the reconciliation are computed the same way. The walker emits
                // in source preorder, so a node's descendants are the contiguous
                // run of later lines whose span lies inside its own.
                let below: Vec<bool> = match (our_file, our_ids) {
                    (Some(file), Some(line_ids)) if file.len() == line_ids.len() => (0..file.len())
                        .map(|i| {
                            if file[i].type_string != "error" {
                                return true;
                            }
                            let outer = nodes.span(line_ids[i]);
                            !(i + 1..file.len())
                                .take_while(|&j| {
                                    let inner = nodes.span(line_ids[j]);
                                    inner.start >= outer.start && inner.end <= outer.end
                                })
                                .any(|j| file[j].type_string == "error")
                        })
                        .collect(),
                    _ => Vec::new(),
                };

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
                        | TypeData::BooleanLiteral(_)
                        | TypeData::EnumLiteral { .. } => Shape::Literal,
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
                        *report.intrinsics.entry(printed.clone()).or_default() += 1;
                    }

                    // THE RECONCILIATION. `rank_board` keys each row by
                    // `(row_key, cause)` and prints its rankings per cause, so
                    // its 4,005 may be row 6's *propagated/span* count rather
                    // than row 6's total. Bucketing by the same span test says
                    // so or refutes it.
                    //
                    // **The polarity is the trap and it caught me.** `below[i]`
                    // is `true` when *nothing* gapped below — `rank_board` names
                    // the binding `gapped_below` and then passes `!gapped_below`
                    // into `cause()`. Copying the expression without its use
                    // inverted the whole split, and the smoke run reported row 5
                    // as 100% propagated and row 6 as 100% terminal, which is
                    // exactly backwards. The control below is what makes that
                    // self-detecting rather than a plausible-looking table.
                    let line_cause = match below.get(position) {
                        Some(true) => Cause::Terminal,
                        // No span data is not evidence of TERMINAL. `rank_board`
                        // counts that case as UNATTRIBUTED and its control reads
                        // zero, so the arm is unreachable in practice and is
                        // written to fail loudly rather than to guess.
                        Some(false) => Cause::PropagatedSpan,
                        None => {
                            report.control_not_access += 1;
                            continue;
                        }
                    };
                    *report.causes.entry((row, line_cause)).or_default() += 1;
                    if row == Row::Access
                        && line_cause == Cause::Terminal
                        && report.terminal_examples.len() < 8
                    {
                        report.terminal_examples.push((case.name.clone(), got.text.clone()));
                    }

                    // PROBE PAST THE BLOCKER, for the composite receivers.
                    // `docs/conventions.md`: a probe at the blocker only shows
                    // it is live; the question is what the form answers once it
                    // is removed. Strip the nullish constituents upstream's
                    // `checkNonNullExpression` removes before the lookup, then
                    // ask each remaining constituent for the name.
                    if shape == Shape::Composite {
                        if access.question_dot_token.is_some() {
                            report.optional_chain += 1;
                        }
                        let (constituents, is_union) = match &checker.type_of(receiver_type).data {
                            TypeData::Union { types, .. } => (types.clone(), true),
                            TypeData::Intersection { types, .. } => (types.clone(), false),
                            _ => (Vec::new(), true),
                        };
                        let kept: Vec<_> = constituents
                            .iter()
                            .copied()
                            .filter(|&constituent| {
                                !matches!(
                                    &checker.type_of(constituent).data,
                                    TypeData::Intrinsic { name: "undefined" | "null" }
                                )
                            })
                            .collect();
                        if kept.len() != constituents.len() {
                            report.nullish_in_union += 1;
                        }
                        let found = kept
                            .iter()
                            .filter(|&&constituent| {
                                checker.get_property_of_type(constituent, name.text).is_some()
                            })
                            .count();
                        // Upstream's rule, `getPropertyOfUnionOrIntersectionType`:
                        // a union answers only if every constituent has the
                        // property; an intersection if any does.
                        let outcome = distributed_outcome(is_union, found, kept.len());
                        *report.distributed.entry(outcome).or_default() += 1;

                        // And upstream's own answer for the receiver position,
                        // which is what says whether our union is even the type
                        // upstream had there. `kind`/`type` on a union in files
                        // named after discriminant narrowing is a narrowing
                        // signature until this says otherwise.
                        let their_type = receiver
                            .node_id()
                            .and_then(|receiver_id| position_of.get(&receiver_id))
                            .and_then(|&receiver_position| {
                                let their = expected_file.assertions.get(receiver_position)?;
                                let ours = our_file.and_then(|file| file.get(receiver_position))?;
                                their
                                    .text
                                    .strip_prefix(&format!("{} : ", ours.text))
                                    .map(str::to_string)
                            });
                        let verdict = their_type
                            .as_ref()
                            .map_or(Agreement::NoComparison, |their| agreement(their, &printed));
                        *report.agreements.entry(verdict).or_default() += 1;
                        if let Some(their) = their_type
                            && report.receiver_pairs.len() < 200
                        {
                            *report.receiver_pairs.entry((their, printed.clone())).or_default() +=
                                1;
                        }
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
    let mut causes: HashMap<(Row, Cause), usize> = HashMap::new();
    let mut distributed: HashMap<Distributed, usize> = HashMap::new();
    let mut receiver_pairs: HashMap<(String, String), usize> = HashMap::new();
    let mut agreements: HashMap<Agreement, usize> = HashMap::new();
    let mut terminal_examples: Vec<(String, String)> = Vec::new();
    let (mut optional_chain, mut nullish_in_union) = (0usize, 0usize);
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
        for (key, count) in &case.causes {
            *causes.entry(*key).or_default() += count;
        }
        for (key, count) in &case.distributed {
            *distributed.entry(*key).or_default() += count;
        }
        for (key, count) in &case.agreements {
            *agreements.entry(*key).or_default() += count;
        }
        for (key, count) in &case.receiver_pairs {
            *receiver_pairs.entry(key.clone()).or_default() += count;
        }
        terminal_examples.extend(case.terminal_examples.iter().cloned());
        optional_chain += case.optional_chain;
        nullish_in_union += case.nullish_in_union;
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
    println!("RECONCILIATION WITH rank_board — rows 5/6 by rank_board's own cause split");
    println!("  the board prints its rankings PER CAUSE, so its 4,005 may be row 6's");
    println!("  propagated/span count and not row 6's total. Measured:");
    for row in [Row::MemberName, Row::Access] {
        let label = match row {
            Row::MemberName => "row 5 (member name)",
            Row::Access => "row 6 (property access)",
        };
        for kind in [Cause::Terminal, Cause::PropagatedSpan] {
            let count = causes.get(&(row, kind)).copied().unwrap_or_default();
            println!("      {count:>7}  {label:<24} {}", kind.label());
        }
    }
    println!("  row 5 is a LEAF — nothing can be nested inside `b`, so a non-zero");
    println!("  propagated/span count for it means the span test's polarity is inverted.");
    println!("  the row-6 TERMINAL lines, named (up to 8):");
    for (case, text) in terminal_examples.iter().take(8) {
        println!("      {case}:  {text}");
    }

    println!();
    println!("PROBE PAST THE BLOCKER — what a DISTRIBUTED lookup would answer");
    println!("  (composite receivers only; nullish constituents stripped first, as");
    println!("   upstream's checkNonNullExpression does before the lookup)");
    let composite: usize = distributed.values().sum();
    for outcome in Distributed::all() {
        let count = distributed.get(&outcome).copied().unwrap_or_default();
        println!("      {count:>7}  {:>5.1}%  {}", pct(count, composite), outcome.label());
    }
    println!("      {optional_chain:>7}  of those are an optional chain `a?.b`");
    println!("      {nullish_in_union:>7}  of those had `undefined` or `null` in the union");
    println!("  upstream's type for the receiver POSITION against ours, classified:");
    for verdict in Agreement::all() {
        let count = agreements.get(&verdict).copied().unwrap_or_default();
        println!("      {count:>7}  {:>5.1}%  {}", pct(count, composite), verdict.label());
    }
    println!("  and the raw pairs (top 12; the split on ` | ` above is a heuristic):");
    let mut pairs: Vec<_> = receiver_pairs
        .iter()
        .map(|((theirs, ours), count)| (*count, theirs.clone(), ours.clone()))
        .collect();
    pairs.sort_unstable_by(|a, b| b.cmp(a));
    for (count, theirs, ours) in pairs.iter().take(12) {
        let verdict = if theirs == ours { "SAME" } else { "DIFFERS" };
        println!("      {count:>6}  {verdict:<8} upstream {theirs}  |  ours {ours}");
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
    // The sixth control, added after the span test's polarity was copied
    // inverted and produced a table that looked entirely plausible: row 5 is the
    // `b` of `a.b`, a leaf, so no line can be nested strictly inside its span
    // and `propagated/span` is impossible for it by construction. This reads
    // 4,016 rather than 0 the moment the polarity flips.
    println!(
        "  CONTROL row 5 (a LEAF) counted as propagated/span    = {}",
        causes.get(&(Row::MemberName, Cause::PropagatedSpan)).copied().unwrap_or_default()
    );
}
