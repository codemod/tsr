//! Every gap line walked to its **root** — the gap with no gap under it.
//!
//! `cargo run --release -p tsr-conformance --example gaproot`
//!
//! # Why this exists
//!
//! `examples/rank_board.rs` classifies each gap line by *one step* of evidence:
//! is something it depends on also a gap? Corrected on 2026-08-06 (`b5decc5`),
//! that split reads **16.41% TERMINAL / 24.77% propagated-span / 13.79%
//! propagated-named / 33.06% DEPENDENT-UNKNOWN / 11.97% UNMATCHED**. So the
//! board ranks **symptoms**: five sixths of the gap is a line waiting on
//! something else, and no ranking says what.
//!
//! This probe takes the second step, and every step after it. For each gap line
//! it follows the dependency the *measured* reason names — never an assumed one
//! — until it reaches a node with no gap under it, and attributes the line to
//! **that** node's row. The ranking is then by *root*, and the number beside a
//! row is the one nobody had: **if this root were fixed, how many gap lines stop
//! being gaps?**
//!
//! It generalises `examples/receiver_gap.rs`, which walks exactly one
//! propagation form (`the receiver is a gap`) to its declaration. The forms here
//! are five:
//!
//! | edge | from | to | why it is sound |
//! |---|---|---|---|
//! | `member-of-access` | the `b` of `a.b` | the access `a.b` | `gap_reason` types the member name *as the access* and says so (`types_producer.rs`, "The `b` of `a.b` is typed as the access itself"). The two lines are one failure counted twice. |
//! | `receiver` | `a.b` | `a` | `access_reason` measured `a`'s type as `error`. `receiver_gap.rs`'s edge. |
//! | `span` | a node | the outermost-leftmost rendered line strictly inside its span that gapped | `rank_board`'s `Below` test, both fields. |
//! | `initialiser` | a declaration name / reference | the value declaration's initialiser | `gap_reason` reports `/ initialiser K` only when `getTypeOfSymbol` took that half (`checker.go:16652`). |
//! | *(stop)* `annotation` | a declaration name / reference | the annotation type node | Reported as a **root of its own kind**, not descended — see the limit below. |
//!
//! # The limit, stated before any number
//!
//! **A type node is where this instrument stops.** `/ annotation ArrayType of
//! TypeReference unresolved: Array` names a type node, and this probe files the
//! line under that type node as its root. It does *not* ask whether the type
//! node is itself downstream of another gap — a `TypeReference` naming a type
//! alias whose right-hand side gaps would be. Descending needs declared-type
//! machinery `gap_reason` does not expose, so those roots are **ceilings for the
//! type-node family, not proofs of rootness**. Same for `type declaration name,
//! the alias RHS gaps: X`. Recorded as `open` in the notes and filed.
//!
//! # `ROOT` earns it; `UNMATCHED` is the default
//!
//! `docs/conventions.md`: *never let the semantically loaded label be the
//! default arm*. Every [`Root`] variant except [`Root::Unmatched`] carries a
//! positive test — `Root::OwnRule` requires `has_inner`, exactly as
//! `rank_board`'s `Terminal` does after its correction. A line no arm claims
//! reaches `Unmatched`, which is a control and can fire.
//!
//! # The baseline is tested first
//!
//! `examples/reconcile.rs` measured 389 lines where we answer `error` **and
//! upstream's baseline also says `error`** being filed as gaps by a probe that
//! asked `type_string == "error"` first. Those are right answers. The order here
//! is baseline, then gap.

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// How deep a dependency chain may go before the line is filed as
/// [`Root::DepthBound`] rather than walked further.
///
/// A silent truncation reads as *"we traced everything"* when it did not, so the
/// bound is counted and printed as its own bucket
/// (`docs/conventions.md`). `receiver_gap`'s deepest measured chain is 6.
const MAX_DEPTH: usize = 64;

/// **RULE-2's B2 switch.** With it off, this probe is the instrument that
/// produced Part 1 of `checker-notes-gaproot.md`; with it on, a
/// `the property has no type` access descends to the property's own
/// declaration instead of claiming to be its own root. Both runs are reported,
/// because the difference **is** the measurement — the same shape as
/// `checker-notes-rank.md`'s M0.
const PROPERTY_DECLARATION_EDGE: bool = true;

/// What stopped the walk. **Every arm but [`Root::Unmatched`] has a positive
/// test**; `Unmatched` is the default and is the control.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Root {
    /// The node has at least one rendered line strictly inside its span and
    /// **none of them gapped**. Everything it is built from was typed, so its
    /// own rule is the whole of the missing work. The positive test is
    /// `has_inner`, without which a leaf claims this label on no evidence —
    /// the defect `rank_board` was corrected for.
    OwnRule,
    /// The reason names a **type node** — an annotation, or a type alias's
    /// right-hand side. A root by the limit of this instrument, not by proof.
    TypeNode,
    /// The symbol has no value declaration at all: nothing for
    /// `getTypeOfSymbol` to dispatch *on*. A binder item.
    NoValueDeclaration,
    /// The name did not resolve in value position. A binder/lib item.
    NameUnresolved,
    /// `gap_reason` says the symbol *has* a type — the descent reached a node
    /// that does not gap, so the chain ended for a different cause.
    SymbolHasType,
    /// A dependency was named and could not be reached: no access parent, no
    /// receiver node, no value declaration node, no initialiser id. A **positive
    /// failure of the walk**, not a residue.
    BrokenDescent,
    /// [`MAX_DEPTH`] steps without terminating.
    DepthBound,
    /// The walk returned to a node it had already visited.
    Cycle,
    /// **The control.** No arm matched: the reason names no dependency and the
    /// node has nothing inside its span to ask about.
    Unmatched,
}

impl Root {
    const fn label(self) -> &'static str {
        match self {
            Self::OwnRule => "ROOT/own-rule",
            Self::TypeNode => "ROOT/type-node",
            Self::NoValueDeclaration => "ROOT/no-value-decl",
            Self::NameUnresolved => "ROOT/name-unresolved",
            Self::SymbolHasType => "ROOT/symbol-has-type",
            Self::BrokenDescent => "BROKEN-DESCENT",
            Self::DepthBound => "DEPTH-BOUND",
            Self::Cycle => "CYCLE",
            Self::Unmatched => "UNMATCHED",
        }
    }

    /// Whether this is a claim about work, as opposed to a limit of the walk.
    const fn is_root(self) -> bool {
        matches!(
            self,
            Self::OwnRule
                | Self::TypeNode
                | Self::NoValueDeclaration
                | Self::NameUnresolved
                | Self::SymbolHasType
        )
    }
}

/// The volatile tail of a reason, removed so a row is a row and not a type.
/// Verbatim from `rank_board::row_key`, so the two boards' keys line up.
fn row_key(reason: &str) -> String {
    for cut in ["has no such property: ", "unresolved: "] {
        if let Some(at) = reason.find(cut) {
            return reason[..at + cut.len() - 2].to_string();
        }
    }
    reason.to_string()
}

/// `rank_board::family`, copied verbatim including its arm order, so this
/// probe's roll-up reconciles line-for-line with the board's rather than
/// approximating it. The order is load-bearing: several reasons satisfy more
/// than one test.
fn family(reason: &str) -> &'static str {
    if reason.starts_with("type declaration name") {
        "a type alias whose right-hand side gaps"
    } else if reason.starts_with("neither a declaration name nor an expression") {
        "a node that is neither a declaration name nor an expression"
    } else if reason.contains("symbol has a type (") {
        "the symbol has a type; the line differs for another reason"
    } else if reason.contains("the receiver is a gap") {
        "a property access whose receiver we cannot type"
    } else if reason.starts_with("property access") || reason.starts_with("member name,") {
        "a property access whose property we cannot find or type"
    } else if reason.contains("annotation") {
        "a type node we cannot resolve"
    } else if reason.contains("initialiser") {
        "an initialiser expression we do not compute"
    } else if reason.starts_with("expression answered error") {
        "AN EXPRESSION WE DO NOT COMPUTE  <- the family under test"
    } else if reason.contains("the name of a") {
        "a member name, resolved as if it were free"
    } else if reason.contains("does not resolve") {
        "a free name that does not resolve, in value position"
    } else if reason.contains("no value declaration") {
        "a symbol with no value declaration at all"
    } else if reason.contains("neither") {
        "a symbol whose kind getTypeOfSymbol does not handle"
    } else {
        "UNCLASSIFIED"
    }
}

/// `rank_board::cause`, copied verbatim, used **only** as a cross-instrument
/// control (C4). Not part of this probe's classification.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BoardCause {
    Terminal,
    PropagatedSpan,
    PropagatedNamed,
    Unknown,
    Unmatched,
}

fn board_cause(reason: &str, inner_gapped: bool, has_inner: bool) -> BoardCause {
    if reason.contains("the receiver is a gap") {
        BoardCause::PropagatedNamed
    } else if inner_gapped {
        BoardCause::PropagatedSpan
    } else if reason.contains("/ initialiser ")
        || reason.contains("/ annotation ")
        || reason.contains("no value declaration")
        || reason.contains("the name does not resolve")
    {
        BoardCause::Unknown
    } else if has_inner {
        BoardCause::Terminal
    } else {
        BoardCause::Unmatched
    }
}

/// What one root row holds, within one case.
#[derive(Default, Clone)]
struct Tally {
    lines: usize,
    /// Sum of descent depths, for the mean.
    depth_sum: usize,
    /// Lines that are their own root — depth 0.
    own: usize,
    /// Baseline right-hand sides for the lines this root blocks. **The
    /// spellability check**: a root that unblocks lines this port cannot *name*
    /// converts gaps into wrong lines.
    rhs: HashMap<String, usize>,
    /// Blocked lines whose baseline RHS is a form this port has no route to
    /// naming: `typeof x` prints a module symbol's file path here, and
    /// `import("m").W` is the corpus's family for symbols with no accessible
    /// name (`docs/conventions.md`, the module-object measurement).
    unnameable: usize,
    /// Blocked lines whose baseline RHS is `any` — closable only by widening,
    /// which `checker-notes-rank.md` §6 forbids.
    any_rhs: usize,
}

impl Tally {
    fn merge(&mut self, other: &Self) {
        self.lines += other.lines;
        self.depth_sum += other.depth_sum;
        self.own += other.own;
        self.unnameable += other.unnameable;
        self.any_rhs += other.any_rhs;
        for (key, n) in &other.rhs {
            *self.rhs.entry(key.clone()).or_default() += n;
        }
    }
}

/// A root row rolled up over the corpus, with the case spread beside it.
#[derive(Default)]
struct Row {
    tally: Tally,
    by_case: HashMap<String, usize>,
}

impl Row {
    #[allow(clippy::cast_precision_loss)]
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.by_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let top1 = counts.first().copied().unwrap_or_default();
        let top10: usize = counts.iter().take(10).sum();
        (counts.len(), pct(top1, self.tally.lines), pct(top10, self.tally.lines))
    }

    fn top_cases(&self, n: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> =
            self.by_case.iter().map(|(case, count)| (case.clone(), *count)).collect();
        entries.sort_by_key(|(case, count)| (std::cmp::Reverse(*count), case.clone()));
        entries.truncate(n);
        entries
    }
}

#[derive(Default)]
struct CaseReport {
    name: String,
    expected: usize,
    right: usize,
    gap: usize,
    wrong: usize,
    aligned: usize,
    unaligned: usize,
    /// Span steps that skipped a gapped child because it was the current
    /// access's own **member name**. Not an operand: `gap_reason` types the
    /// member name *as* the access, so descending into it walks straight back
    /// out through the `member-of-access` edge. Before this exclusion the walk
    /// reported **20,232 lines (14.19%) as CYCLE**, every one of them that loop.
    span_skipped_member_name: usize,
    /// The lib-name signal `row_key` cuts out of a type-node row: for a
    /// `TypeReference unresolved: X` root, the `X`.
    unresolved_names: HashMap<String, usize>,
    /// **RULE-2's B1.** For every line whose root is a property-access row,
    /// whether the root access's *receiver* is typed the way upstream types it.
    /// Keyed by `(row, verdict)`; the unit is gap assertion lines.
    ///
    /// A receiver that is typed **wrongly** does not gap, so nothing inside the
    /// access's span gapped, so `gaproot` calls the access its own root — and
    /// the lookup that failed did so on a type `members.rs` was never given a
    /// chance to search. Those lines are not this row's work at any size.
    receiver_fidelity: HashMap<(String, &'static str), usize>,
    /// The receiver types the lookup actually failed on, when the receiver is
    /// typed correctly. This is what names the work inside the row.
    receiver_types: HashMap<String, usize>,
    /// For the **primitive-receiver** slice only — the one that is `members.rs`
    /// work — what upstream prints for the line. `getApparentType` makes the
    /// lookup *reach* `String`/`Number`; it does not make this port able to
    /// compute what it finds there. A slice whose answers are call signatures
    /// converts nothing, and this is the bucket that says which.
    primitive_rhs: HashMap<String, usize>,
    /// The same slice, by case, so its concentration can be read separately
    /// from the whole row's.
    primitive_by_case: HashMap<String, usize>,
    /// **RULE-3's P1.** Every gap line whose *node kind* is
    /// `PropertyAccessExpression`, split by what its **receiver** did. Keyed by
    /// the arm; the unit is gap assertion lines.
    ///
    /// The population is pinned by the AST — a node's kind cannot move under
    /// the checker — which is `fnexpr.rs`'s shape and is why `|P|` here is a
    /// constant to compare against `checker-notes-wrong.md`'s 15,215 rather
    /// than a number this probe produces.
    access_arms: HashMap<&'static str, usize>,
    /// The `RIGHT` arm only, by case, for P1b.
    access_right_by_case: HashMap<String, usize>,
    /// The `RIGHT` arm only: what upstream prints, for P1c.
    access_right_rhs: HashMap<String, usize>,
    /// The `RIGHT` arm only, by receiver kind, so the work items inside it are
    /// separable exactly as they were in Part 2.
    access_right_family: HashMap<&'static str, usize>,
    /// **The cross-file merge split.** For `RIGHT`-arm lines whose receiver is a
    /// plain named type: does a global of that name have declarations in more
    /// than one **file**?
    ///
    /// This is the pass the naming agent specified, and it is the one that says
    /// whether the merge asymmetry is a large item or a small one. Keyed by the
    /// verdict; unit is gap assertion lines.
    merge_split: HashMap<&'static str, usize>,
    /// The same verdict crossed with the receiver kind, so a reader can see
    /// which sub-item each verdict belongs to rather than inferring it.
    merge_by_family: HashMap<(&'static str, &'static str), usize>,
    /// **The `this` spec, population A.** Gap lines whose node kind is
    /// `ThisKeyword`, keyed by the **this-container** upstream would find.
    /// Pinned syntactically: the kind is an AST fact.
    this_population: HashMap<&'static str, usize>,
    /// What upstream prints for those lines, per container — the spellability
    /// leg, kept per container because the answers differ completely between
    /// them (`this` vs `typeof C` vs `typeof globalThis`).
    this_rhs: HashMap<(&'static str, String), usize>,
    /// **RULE-6.** For every gap line in the family under test: the family its
    /// ROOT belongs to. This is the transition matrix that answers whether the
    /// largest family is downstream of the others.
    expr_to_root_family: HashMap<(&'static str, bool), usize>,
    /// The same lines by what the expression syntactically **is**, crossed with
    /// whether the line is its own root.
    expr_by_kind: HashMap<(String, &'static str), usize>,
    /// The root KIND (own-rule / type-node / …) within the family.
    expr_root_kind: HashMap<(&'static str, bool), usize>,
    /// TS2563 lines in the family, by case, so the concentration claim is
    /// measured rather than asserted.
    expr_ts2563_by_case: HashMap<String, usize>,
    /// **RULE-7.** For the two node kinds under test, the baseline's exact
    /// right-hand side, keyed by `(kind, own root?, TS2563?)`. Bounded per case.
    e7_rhs: HashMap<(&'static str, bool, bool, String), usize>,
    /// **Uncapped totals for the same populations.**
    ///
    /// `e7_rhs` is capped per case, and on the first run that cap silently
    /// dropped increments once a case had many distinct type texts: the
    /// propagated `ElementAccessExpression` row read **1,048** against Part 8's
    /// independently-derived **1,540**, while the own-root row matched to the
    /// line. A cap that drops counts reads as *"we counted everything"*, which
    /// is the failure `docs/conventions.md` records for silent truncation. The
    /// histogram stays capped — distinct type texts are unbounded — and the
    /// **totals are counted here where nothing can drop them**.
    e7_totals: HashMap<(&'static str, bool, bool), usize>,
    /// How many histogram increments the cap dropped, so the truncation is a
    /// printed number rather than an absence.
    e7_dropped: usize,
    /// **E2 for object literals.** Which member form makes
    /// `check_object_literal` bail, which is what names the owner.
    e7_object_member: HashMap<&'static str, usize>,
    /// **E1 re-measured over the SPREAD subset alone**, which is the condition
    /// registered before `bd tsr-sps` may be built: the 5.9% `any` and 0.0%
    /// unnameable are over all 575 and the 156 may differ.
    e7_spread_rhs: HashMap<String, usize>,
    e7_spread_by_case: HashMap<String, usize>,
    /// The same lines by case, for the concentration leg.
    e7_by_case: HashMap<(&'static str, String), usize>,
    /// **RULE-5.** The three remaining `members.rs` questions, each keyed by
    /// what the sub-item actually turns out to be.
    left_n22: HashMap<&'static str, usize>,
    left_base: HashMap<&'static str, usize>,
    left_name: HashMap<&'static str, usize>,
    /// **The `this` spec, population B.** The 1,165 `RIGHT`-arm lines whose
    /// *receiver* is `this` and whose member lookup failed, keyed by why.
    this_receiver: HashMap<&'static str, usize>,
    roots: HashMap<(Root, String), Tally>,
    depth_hist: BTreeMap<usize, usize>,
    /// How many *distinct top-level* gapped children a span step chose from. 1
    /// means the attribution is forced; more means this probe attributes to the
    /// first and the others are not credited.
    branch_hist: BTreeMap<usize, usize>,
    edges: BTreeMap<&'static str, usize>,
    /// The board's one-step cause, so this probe's population reconciles with
    /// `rank_board`'s published split.
    board: BTreeMap<&'static str, usize>,
    c1_root_names_dependency: usize,
    c1_root_names_none: usize,
    c2_own_rule_on_leaf: usize,
    c2_own_rule_has_inner: usize,
    c3_member_without_access: usize,
    c3_member_with_access: usize,
    c4_board_terminal_deep: usize,
    c4_board_terminal_depth0: usize,
    /// Descent steps that landed on a node the walker rendered no line for, so
    /// no span evidence was available and the reason alone decided. A
    /// measurement, not a zero.
    c5_steps_without_span_data: usize,
}

impl CaseReport {
    fn edge(&mut self, name: &'static str) {
        *self.edges.entry(name).or_default() += 1;
    }
}

/// What a rendered line's span holds.
struct Inner {
    /// There was at least one rendered line strictly inside this node's span.
    /// **Two facts, not one**: *"nothing inside gapped"* and *"there is nothing
    /// inside"* are the same empty `gapped` and mean opposite things.
    has_inner: bool,
    /// The top-level gapped children, in source order. Nested-inside-a-gap
    /// children are excluded, so the branch count is the number of *independent*
    /// dependencies rather than the size of the subtree.
    gapped: Vec<usize>,
}

/// One descent step's outcome.
enum Step {
    Go(NodeId, &'static str),
    Stop(Root, String),
}

fn main() {
    check_classifier();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let reports: Vec<CaseReport> = cases.par_iter().filter_map(measure).collect();
    report(&reports);
}

/// The classifier's load-bearing properties, checked on **every run** before
/// anything is measured — assertions in `main` rather than `#[cfg(test)]`
/// because Cargo does not run tests inside an example without a manifest change
/// and `crates/tsr-conformance/Cargo.toml` is shared with three other agents
/// this cycle.
fn check_classifier() {
    // ROOT HAS TO EARN IT. A leaf has nothing inside it, so an empty `gapped`
    // is the absence of a test and not a passing one.
    assert_eq!(
        classify_stop("expression answered error: ArrowFunction", false).0,
        Root::Unmatched,
        "a leaf with no named dependency reaches the control, not a ROOT"
    );
    assert_eq!(
        classify_stop("expression answered error: ArrowFunction", true).0,
        Root::OwnRule,
        "a node with typed children and no named dependency is its own rule"
    );
    // THE TYPE-NODE STOP. Both forms name a type node and neither is descended.
    assert_eq!(
        classify_stop(
            "declaration name, symbol has no type: SymbolFlags(X) / VariableDeclaration \
             / annotation TypeReference unresolved: Array",
            false
        ),
        (Root::TypeNode, "annotation TypeReference unresolved".to_string()),
        "the type name is cut from the row key, exactly as `rank_board` cuts it"
    );
    assert_eq!(
        classify_stop("type declaration name, the alias RHS gaps: TypeLiteral", false),
        (Root::TypeNode, "alias RHS TypeLiteral".to_string()),
    );
    // THE TWO BINDER ROOTS, which `rank_board` lumps into DEPENDENT-UNKNOWN.
    assert_eq!(
        classify_stop(
            "reference, symbol has no type: SymbolFlags(ALIAS) / no value declaration",
            false
        )
        .0,
        Root::NoValueDeclaration
    );
    assert_eq!(
        classify_stop("reference, the name does not resolve", false).0,
        Root::NameUnresolved
    );
    // KEY COLLAPSE, verbatim from `rank_board` so the two boards' rows line up.
    assert_eq!(
        row_key("property access, the receiver has no such property: Promise<boolean>"),
        row_key("property access, the receiver has no such property: SymbolConstructor"),
    );
    assert_ne!(
        row_key("property access, the receiver is a gap: Identifier"),
        row_key("property access, the receiver is a gap: PropertyAccessExpression"),
    );
    // THE CROSS-INSTRUMENT CONTROL'S OWN PREMISE. C4 asserts that every line the
    // board calls TERMINAL stops here at depth 0; that only means something if
    // `board_cause` still answers TERMINAL for such a line.
    assert_eq!(
        board_cause("expression answered error: ElementAccessExpression", false, true),
        BoardCause::Terminal
    );
    assert_eq!(
        board_cause("member name, the receiver is a gap: Identifier", false, false),
        BoardCause::PropagatedNamed
    );
}

/// The terminal arms, split out so [`check_classifier`] can exercise them
/// without a corpus. `has_inner` is [`Root::OwnRule`]'s positive test.
fn classify_stop(reason: &str, has_inner: bool) -> (Root, String) {
    // `row_key` on the type-node key too, so `annotation TypeReference
    // unresolved: Emitter` and `… : AST` are one row rather than 90. The names
    // are not thrown away — they are the direct test of *"is this a lib type"*
    // (`bd tsr-9or.1`) — they are reported in their own table instead.
    if let Some(annotation) = reason.split_once("/ annotation ") {
        return (Root::TypeNode, row_key(&format!("annotation {}", annotation.1)));
    }
    if let Some(alias) = reason.split_once("the alias RHS gaps: ") {
        return (Root::TypeNode, row_key(&format!("alias RHS {}", alias.1)));
    }
    if reason.contains("no value declaration") {
        return (Root::NoValueDeclaration, row_key(reason));
    }
    if reason.contains("the name does not resolve") {
        return (Root::NameUnresolved, row_key(reason));
    }
    if reason.contains("symbol has a type") {
        return (Root::SymbolHasType, row_key(reason));
    }
    if has_inner {
        return (Root::OwnRule, row_key(reason));
    }
    (Root::Unmatched, row_key(reason))
}

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<CaseReport> {
    // The suite's own skips, skip for skip, so every share is a share of the
    // gradient's denominator and not of some other set.
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
    // `with_module_host`, which is what `render_case` itself builds
    // (ADR-0041). `receiver_gap` uses the same; `rank_board` uses
    // `Checker::new`, and the difference is noted in the report.
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
    // **The ADR-0038 ceiling, pinned by the case's own `.errors.txt`.** Under
    // TS2563 upstream *disables* control-flow analysis and answers `any`, so
    // these lines are unreachable by any checker work. Printed as its own
    // count, never netted away (`checker-notes-evolvearray.md`).
    let ts2563 =
        case.expected_errors().ok().flatten().is_some_and(|errors| errors.contains("TS2563"));

    let mut report = CaseReport { name: case.name.clone(), ..CaseReport::default() };

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        let mut position_of: HashMap<NodeId, usize> = HashMap::new();
        for (position, id) in line_ids.iter().enumerate() {
            position_of.entry(*id).or_insert(position);
        }

        for (position, want) in expected_file.assertions.iter().enumerate() {
            report.expected += 1;
            let Some(got) = our_file.get(position) else {
                // Upstream asserts a line we rendered nothing for at all.
                report.unaligned += 1;
                continue;
            };
            // **Baseline first.** See the module docs: asking `type_string ==
            // "error"` first files 389 right answers as gaps.
            if want.text == got.line() {
                report.aligned += 1;
                report.right += 1;
                continue;
            }
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                // **Unaligned, and therefore not this probe's population.**
                // `types_suite::compare` and `rank_board` both compare types
                // only where the walker reproduced upstream's expression text;
                // a line whose text we got wrong fails its case for a walker
                // reason and no checker work touches it. Counting it as a gap
                // is what made this probe's first run report 142,593 gap lines
                // against the board's 138,585.
                report.unaligned += 1;
                continue;
            };
            report.aligned += 1;
            if got.type_string != "error" {
                report.wrong += 1;
                continue;
            }
            report.gap += 1;
            walk(
                &mut report,
                &mut checker,
                bound,
                nodes,
                map,
                our_file,
                line_ids,
                &position_of,
                &expected_file.assertions,
                &case.name,
                ts2563,
                position,
                want_type,
            );
        }
    }
    Some(report)
}

/// What a node's span holds, over the contiguous preorder run of later lines.
///
/// Containment is inclusive, matching `rank_board`'s `Below` exactly, so C4 is a
/// comparison of two identical predicates rather than of two similar ones.
fn inner_of(
    nodes: &tsr_ast::NodeTable,
    file: &[types_producer::Assertion],
    line_ids: &[NodeId],
    position: usize,
) -> Inner {
    let outer = nodes.span(line_ids[position]);
    let mut has_inner = false;
    let mut gapped = Vec::new();
    let mut covered_end = 0u32;
    for j in position + 1..file.len() {
        let inner = nodes.span(line_ids[j]);
        if !(inner.start >= outer.start && inner.end <= outer.end) {
            break;
        }
        has_inner = true;
        if file[j].type_string == "error" && inner.start >= covered_end {
            gapped.push(j);
            covered_end = inner.end;
        }
    }
    Inner { has_inner, gapped }
}

/// Walk one gap line to its root, counting every step.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn walk(
    report: &mut CaseReport,
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    file: &[types_producer::Assertion],
    line_ids: &[NodeId],
    position_of: &HashMap<NodeId, usize>,
    baseline: &[types_baseline::TypeAssertion],
    case_name: &str,
    ts2563: bool,
    start: usize,
    want_type: &str,
) {
    let mut node = line_ids[start];
    let mut visited: Vec<NodeId> = vec![node];
    let mut depth = 0usize;
    let mut board_recorded = false;
    // The line's OWN reason, kept because the root's reason overwrites it and
    // the family under test is defined by the first, not the last.
    let mut first_reason = String::new();

    let (root, key, final_reason, root_node) = loop {
        let reason = types_producer::gap_reason(checker, bound, nodes, map, node);
        if depth == 0 {
            first_reason.clone_from(&reason);
        }
        // The span test, where this node is a rendered line. A node reached
        // through an initialiser edge may not be one; then there is no span
        // evidence and the reason alone decides.
        let inner =
            position_of.get(&node).map(|&position| inner_of(nodes, file, line_ids, position));
        let has_inner = inner.as_ref().is_some_and(|i| i.has_inner);
        // **Unfiltered**, because this is the input `rank_board`'s `Below` gives
        // its `cause()`, and C4 is only a control if the two predicates are the
        // same one.
        let first_gapped = inner.as_ref().and_then(|i| i.gapped.first().copied());
        // Filtered, for the descent: the access's own member name is not an
        // operand of the access (see `span_skipped_member_name`).
        let descend_gapped: Vec<usize> = inner.as_ref().map_or_else(Vec::new, |inner| {
            inner
                .gapped
                .iter()
                .copied()
                .filter(|&child| !is_member_name_of(nodes, map, node, line_ids[child]))
                .collect()
        });
        report.span_skipped_member_name +=
            inner.as_ref().map_or(0, |inner| inner.gapped.len() - descend_gapped.len());

        if depth > 0 && inner.is_none() {
            report.c5_steps_without_span_data += 1;
        }
        // C3, read where the phrase is produced rather than inferred from the
        // arm that consumed it.
        if reason.starts_with("member name, ") {
            if nodes
                .parent(node)
                .is_some_and(|parent| nodes.kind(parent) == SyntaxKind::PropertyAccessExpression)
            {
                report.c3_member_with_access += 1;
            } else {
                report.c3_member_without_access += 1;
            }
        }
        if depth == 0 {
            let cause = board_cause(&reason, first_gapped.is_some(), has_inner);
            *report
                .board
                .entry(match cause {
                    BoardCause::Terminal => "TERMINAL",
                    BoardCause::PropagatedSpan => "propagated/span",
                    BoardCause::PropagatedNamed => "propagated/named",
                    BoardCause::Unknown => "DEPENDENT-UNKNOWN",
                    BoardCause::Unmatched => "UNMATCHED",
                })
                .or_default() += 1;
            board_recorded = cause == BoardCause::Terminal;
        }
        if let Some(inner) = inner.as_ref()
            && !inner.gapped.is_empty()
        {
            *report.branch_hist.entry(inner.gapped.len()).or_default() += 1;
        }

        let step = descend(
            checker,
            bound,
            nodes,
            map,
            node,
            &reason,
            descend_gapped.first().copied(),
            has_inner,
            line_ids,
        );
        // C1/C2/C3 are read at the point the walk stops, not inferred from the
        // arm that stopped it.
        match step {
            Step::Stop(root, key) => {
                // Every **classification** stop, not only the ROOT arms:
                // `Unmatched` is included because a descent arm that was
                // deleted lands there, and C1 is the control that has to see
                // that. `BrokenDescent` is excluded — naming a dependency it
                // could not reach is what it *is*.
                if root.is_root() || root == Root::Unmatched {
                    if names_dependency(&reason) {
                        report.c1_root_names_dependency += 1;
                    } else {
                        report.c1_root_names_none += 1;
                    }
                    if root == Root::OwnRule {
                        if has_inner {
                            report.c2_own_rule_has_inner += 1;
                        } else {
                            report.c2_own_rule_on_leaf += 1;
                        }
                    }
                }
                break (root, key, reason, node);
            }
            Step::Go(next, edge) => {
                report.edge(edge);
                if visited.contains(&next) {
                    break (Root::Cycle, row_key(&reason), reason, node);
                }
                if depth + 1 >= MAX_DEPTH {
                    break (Root::DepthBound, row_key(&reason), reason, node);
                }
                visited.push(next);
                node = next;
                depth += 1;
            }
        }
    };

    if board_recorded {
        if depth == 0 {
            report.c4_board_terminal_depth0 += 1;
        } else {
            report.c4_board_terminal_deep += 1;
        }
    }
    // **RULE-7.** The two node kinds under test. Keyed on the line's own node
    // kind (an AST fact), on whether it is its own root, and on TS2563 — so the
    // survivors can be read apart from the excluded lines rather than inferred
    // by subtraction, which is what the 88.4% figure was.
    let e7_kind = match nodes.kind(line_ids[start]) {
        SyntaxKind::ElementAccessExpression => Some("ElementAccessExpression"),
        SyntaxKind::ObjectLiteralExpression => Some("ObjectLiteralExpression"),
        _ => None,
    };
    if let Some(e7_kind) = e7_kind {
        *report.e7_totals.entry((e7_kind, depth == 0, ts2563)).or_default() += 1;
        if report.e7_rhs.len() < 8192 {
            *report
                .e7_rhs
                .entry((e7_kind, depth == 0, ts2563, want_type.to_string()))
                .or_default() += 1;
        } else {
            report.e7_dropped += 1;
        }
        if depth == 0 && !ts2563 {
            *report.e7_by_case.entry((e7_kind, case_name.to_string())).or_default() += 1;
            if e7_kind == "ObjectLiteralExpression"
                && let Some(Node::ObjectLiteralExpression(literal)) = map.get(line_ids[start])
            {
                // Mirrors `check_object_literal`'s dispatch order exactly, so
                // the bucket names the arm that actually returned `error`
                // rather than a shape that resembles it.
                let mut verdict = "all members are plain assignments — another cause";
                for property in literal.properties {
                    verdict = match property {
                        tsr_ast::ObjectLiteralElementLike::PropertyAssignment(a)
                            if a.initializer.is_none() =>
                        {
                            "a property assignment with no initialiser"
                        }
                        tsr_ast::ObjectLiteralElementLike::PropertyAssignment(_) => continue,
                        tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(sh)
                            if sh.object_assignment_initializer.is_some() =>
                        {
                            "shorthand with an assignment initialiser (destructuring)"
                        }
                        tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(_) => {
                            continue;
                        }
                        tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => {
                            "SPREAD `{...a}` — objects.rs + unions.rs, MINE"
                        }
                        tsr_ast::ObjectLiteralElementLike::MethodDeclaration(_) => {
                            "METHOD `{ m() {} }` — needs signatures.rs"
                        }
                        tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(_)
                        | tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(_) => {
                            "ACCESSOR `{ get x() {} }` — needs return-type inference"
                        } // No `_` arm: clippy proved one unreachable, which
                          // means this match covers every
                          // `ObjectLiteralElementLike` variant. **Exhaustiveness
                          // is now compiler-enforced** — a new member form added
                          // to the AST breaks this build instead of silently
                          // joining a catch-all, which is the construction-pinned
                          // version of a control bucket.
                    };
                    break;
                }
                *report.e7_object_member.entry(verdict).or_default() += 1;
                if verdict.starts_with("SPREAD") {
                    *report.e7_spread_rhs.entry(want_type.to_string()).or_default() += 1;
                    *report.e7_spread_by_case.entry(case_name.to_string()).or_default() += 1;
                }
            }
        }
    }
    // **RULE-6.** The family under test, crossed with where its lines root.
    if family(&first_reason) == "AN EXPRESSION WE DO NOT COMPUTE  <- the family under test" {
        *report.expr_to_root_family.entry((family(&final_reason), ts2563)).or_default() += 1;
        *report.expr_root_kind.entry((root.label(), ts2563)).or_default() += 1;
        if ts2563 {
            *report.expr_ts2563_by_case.entry(case_name.to_string()).or_default() += 1;
        } else {
            *report
                .expr_by_kind
                .entry((
                    format!("{:?}", nodes.kind(line_ids[start])),
                    if depth == 0 { "its own root" } else { "propagated" },
                ))
                .or_default() += 1;
        }
    }
    *report.depth_hist.entry(depth).or_default() += 1;
    // **The `this` spec, population A.** Same shape as P1: pinned by node kind,
    // so `|P|` is an AST fact.
    if nodes.kind(line_ids[start]) == SyntaxKind::ThisKeyword {
        let container = this_container_label(nodes, line_ids[start]);
        *report.this_population.entry(container).or_default() += 1;
        if report.this_rhs.len() < 4096 {
            *report.this_rhs.entry((container, want_type.to_string())).or_default() += 1;
        }
    }
    // **RULE-3's P1**, read at the LINE and keyed on the line's own node kind,
    // not at the root and not on the reason. A gap line whose node is a
    // `PropertyAccessExpression` is in the population whatever it roots as, so
    // `|P|` is fixed by the AST before the checker runs.
    if nodes.kind(line_ids[start]) == SyntaxKind::PropertyAccessExpression {
        let (arm, family) =
            access_arm(checker, nodes, map, file, position_of, baseline, line_ids[start]);
        *report.access_arms.entry(arm).or_default() += 1;
        if arm == ACCESS_RIGHT {
            *report.access_right_by_case.entry(case_name.to_string()).or_default() += 1;
            *report.access_right_rhs.entry(want_type.to_string()).or_default() += 1;
            *report.access_right_family.entry(family).or_default() += 1;
            // **Every RIGHT-arm line, not only the named ones.** RULE-4 asks
            // for the 2,842 outside the 2,599 to be split too, and the same
            // verdict function answers for them — a `this` receiver or a
            // union prints a type name that resolves to nothing, which is a
            // real answer rather than an exclusion.
            *report
                .merge_split
                .entry(merge_verdict(checker, bound, nodes, map, line_ids[start]))
                .or_default() += 1;
            *report
                .merge_by_family
                .entry((family, merge_verdict(checker, bound, nodes, map, line_ids[start])))
                .or_default() += 1;
            // RULE-5's three splits, taken over the same RIGHT arm.
            let verdict = merge_verdict(checker, bound, nodes, map, line_ids[start]);
            if verdict.starts_with("declared once, no heritage") {
                *report
                    .left_n22
                    .entry(n22_verdict(bound, nodes, map, line_ids[start]))
                    .or_default() += 1;
            }
            if verdict.starts_with("declared once, HAS") {
                *report
                    .left_base
                    .entry(base_walk_verdict(bound, nodes, map, line_ids[start]))
                    .or_default() += 1;
            }
            // **The `this` spec, population B.** A `this` receiver that reached
            // `RIGHT` was typed correctly, so the failure is downstream of
            // `this` typing — this asks what it actually is.
            if let Some(Node::PropertyAccessExpression(access)) = map.get(line_ids[start])
                && !matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
            {
                *report
                    .left_name
                    .entry(match access.name {
                        Some(tsr_ast::MemberName::PrivateIdentifier(_)) => {
                            "PRIVATE identifier `#x` — parser/binder surface, not the lookup"
                        }
                        None => "no name node at all",
                        Some(_) => "another member-name form",
                    })
                    .or_default() += 1;
            }
            if family.starts_with("this") {
                *report
                    .this_receiver
                    .entry(this_receiver_verdict(checker, bound, nodes, map, line_ids[start]))
                    .or_default() += 1;
            }
        }
    }
    // **RULE-2's B1**, read at the root rather than at the line: is the access
    // whose lookup failed even looking at the right type?
    if final_reason.contains("the receiver has no such property")
        || final_reason.contains("the property has no type")
    {
        let row = row_key(&final_reason);
        let (verdict, receiver_type) =
            receiver_fidelity(checker, nodes, map, file, position_of, baseline, root_node);
        *report.receiver_fidelity.entry((row, verdict)).or_default() += 1;
        if let Some(receiver_type) = receiver_type {
            if receiver_family(&receiver_type).starts_with("primitive")
                || receiver_family(&receiver_type).ends_with("as `string`")
                || receiver_family(&receiver_type).ends_with("as `number`")
            {
                *report.primitive_rhs.entry(want_type.to_string()).or_default() += 1;
                *report.primitive_by_case.entry(case_name.to_string()).or_default() += 1;
            }
            *report.receiver_types.entry(receiver_type).or_default() += 1;
        }
    }
    // The lib-name signal, kept out of the row key but not thrown away.
    if root == Root::TypeNode
        && let Some((_, name)) = final_reason.rsplit_once("unresolved: ")
    {
        *report.unresolved_names.entry(name.to_string()).or_default() += 1;
    }
    let tally = report.roots.entry((root, key)).or_default();
    tally.lines += 1;
    tally.depth_sum += depth;
    if depth == 0 {
        tally.own += 1;
    }
    if tally.rhs.len() < 256 {
        *tally.rhs.entry(want_type.to_string()).or_default() += 1;
    }
    if want_type.starts_with("typeof ") || want_type.contains("import(") {
        tally.unnameable += 1;
    }
    if want_type == "any" {
        tally.any_rhs += 1;
    }
}

/// The loaded arm of [`access_arm`] — **we hold upstream's own type for the
/// receiver and still cannot look the member up**. It carries the positive
/// test; [`ACCESS_NOT_RENDERED`] is the default.
const ACCESS_RIGHT: &str = "RIGHT   — we hold upstream's type and cannot look the member up";
/// The default arm. Named rather than left as an `else` returning the loaded
/// label, which is the defect `rank_board`'s `cause()` was corrected for.
const ACCESS_NOT_RENDERED: &str = "not rendered — no line for the receiver at all";

/// **RULE-3's P1.** What the receiver of a gapped property access did.
///
/// Arms mirror `checker-notes-evolvearray.md`'s `ElementAccessExpression`
/// split one for one, so the two populations can be read side by side:
/// `RIGHT` / `gapped` / `wrong` / `unaligned` / `not rendered`.
///
/// The distinction Part 2's `receiver_fidelity` did not draw is the one that
/// matters here: *"typed DIFFERENTLY"* merges a receiver that **gapped** with
/// one that is **confidently wrong**, and those are different owners — a
/// gapped receiver is somebody's unported rule, a wrong one is somebody's
/// defect. Splitting them is the whole reason this is a second function rather
/// than a parameter on the first.
fn access_arm(
    checker: &mut tsr_checker::Checker<'_, '_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    file: &[types_producer::Assertion],
    position_of: &HashMap<NodeId, usize>,
    baseline: &[types_baseline::TypeAssertion],
    access: NodeId,
) -> (&'static str, &'static str) {
    let _ = nodes;
    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
        return (ACCESS_NOT_RENDERED, "");
    };
    let Some(receiver) = node.expression else { return (ACCESS_NOT_RENDERED, "") };
    let Some(receiver_id) = receiver.node_id() else { return (ACCESS_NOT_RENDERED, "") };
    let Some(&position) = position_of.get(&receiver_id) else {
        return (ACCESS_NOT_RENDERED, "");
    };
    let (Some(ours), Some(want)) = (file.get(position), baseline.get(position)) else {
        return (ACCESS_NOT_RENDERED, "");
    };
    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", ours.text)) else {
        return ("unaligned — the receiver's own text did not reproduce", "");
    };
    // `error` first, because it is a *gap* and not a wrong answer, and the
    // whole gap/wrong separation this project rests on lives in that order.
    if ours.type_string == "error" {
        return ("gapped  — the receiver is a gap; symptom, not this row", "");
    }
    if want_type != ours.type_string {
        return ("wrong   — the receiver is confidently wrong; symptom", "");
    }
    let receiver_type = checker.check_expression(receiver);
    let printed = checker.type_to_string(receiver_type);
    (ACCESS_RIGHT, receiver_family(&printed))
}

/// **RULE-2's B1.** Is the receiver of this failing access typed the way
/// upstream types it?
///
/// Returns the verdict and, when the receiver *is* typed correctly, the type it
/// was typed as — which is what names the work inside the row, since the lookup
/// then failed on a type `members.rs` genuinely could have searched.
///
/// The comparison is against the **baseline's own right-hand side for the
/// receiver's rendered line**, not against anything this port computes twice.
/// `receiver not rendered` is its own verdict rather than being folded into
/// either answer: upstream's walker does not emit a line for every receiver
/// (a `this`, a parenthesised expression), and counting an absent line as
/// agreement is how a probe manufactures a prerequisite.
fn receiver_fidelity(
    checker: &mut tsr_checker::Checker<'_, '_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    file: &[types_producer::Assertion],
    position_of: &HashMap<NodeId, usize>,
    baseline: &[types_baseline::TypeAssertion],
    access: NodeId,
) -> (&'static str, Option<String>) {
    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
        return ("not an access", None);
    };
    let Some(receiver) = node.expression else { return ("no receiver", None) };
    let Some(receiver_id) = receiver.node_id() else { return ("no receiver", None) };
    let _ = nodes;
    let Some(&position) = position_of.get(&receiver_id) else {
        return ("receiver not rendered", None);
    };
    let (Some(ours), Some(want)) = (file.get(position), baseline.get(position)) else {
        return ("receiver not rendered", None);
    };
    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", ours.text)) else {
        return ("receiver line unaligned", None);
    };
    if want_type == ours.type_string {
        let receiver_type = checker.check_expression(receiver);
        ("receiver typed as upstream types it", Some(checker.type_to_string(receiver_type)))
    } else {
        ("receiver typed DIFFERENTLY", None)
    }
}

/// **`bd tsr-n22`, split.** The receiver's type is declared once, has no
/// heritage clause, and the member is absent. Which of the three candidates is
/// it?
///
/// The index-signature arm is the one that decides ownership: upstream's
/// `getPropertyOfType` misses too, and the answer comes from the **index
/// signature** instead (`crate::index_signatures`, which this workstream owns).
/// The others are the binder's table or the static side.
fn n22_verdict(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    access: NodeId,
) -> &'static str {
    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
        return "not an access";
    };
    let Some(receiver) = node.expression.as_ref().and_then(tsr_ast::Expression::node_id) else {
        return "no receiver";
    };
    let Some(Node::Identifier(name)) = map.get(receiver) else {
        return "receiver is not a bare name — unclassified";
    };
    let symbol = name
        .node_id
        .and_then(|id| bound.resolve_name(nodes, map, id, name.text, SymbolFlags::VALUE))
        .or_else(|| {
            name.node_id
                .and_then(|id| bound.resolve_name(nodes, map, id, name.text, SymbolFlags::TYPE))
        });
    let Some(symbol) = symbol else { return "receiver name does not resolve" };
    // Does the declaration carry an index signature? That is the arm this
    // workstream owns, and it is the only one of the three candidates that is
    // decidable from the AST alone.
    let has_index_signature = bound.symbols().get(symbol).members.values().any(|&member| {
        bound
            .symbols()
            .get(member)
            .declarations
            .iter()
            .any(|&d| nodes.kind(d) == SyntaxKind::IndexSignature)
    });
    if has_index_signature {
        return "INDEX SIGNATURE on the receiver — index_signatures.rs, MINE";
    }
    if bound.symbols().get(symbol).exports.contains_key("") {
        return "static side";
    }
    "no index signature and no base — the binder's table"
}

/// **The base-type walk, split by why `base_symbols_of` gave up.**
///
/// `crate::members` documents exactly three reasons and they have three
/// different owners, so the row is only a work item to the extent the first two
/// dominate: a base with **type arguments** is `bd tsr-4qx`, a base that is
/// **not a plain identifier** is `resolveEntityName`/construct signatures, and a
/// base that **resolves to something with no members** is the alias surface.
fn base_walk_verdict(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    access: NodeId,
) -> &'static str {
    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
        return "not an access";
    };
    let Some(receiver) = node.expression.as_ref().and_then(tsr_ast::Expression::node_id) else {
        return "no receiver";
    };
    let Some(Node::Identifier(name)) = map.get(receiver) else {
        return "receiver is not a bare name — unclassified";
    };
    let symbol = name
        .node_id
        .and_then(|id| bound.resolve_name(nodes, map, id, name.text, SymbolFlags::TYPE))
        .or_else(|| {
            name.node_id
                .and_then(|id| bound.resolve_name(nodes, map, id, name.text, SymbolFlags::VALUE))
        });
    let Some(symbol) = symbol else { return "receiver name does not resolve" };
    let declarations = bound.symbols().get(symbol).declarations.clone();
    for declaration in declarations {
        let clauses = match map.get(declaration) {
            Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
            Some(Node::ClassExpression(node)) => node.heritage_clauses,
            Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
            _ => continue,
        };
        for clause in clauses {
            if clause.token.kind != SyntaxKind::ExtendsKeyword {
                continue;
            }
            for base in clause.types {
                if !base.type_arguments.is_empty() {
                    return "base has TYPE ARGUMENTS — bd tsr-4qx, not members.rs";
                }
                let Some(tsr_ast::Expression::Identifier(base_name)) = base.expression else {
                    return "base is not a plain identifier — resolveEntityName / construct sigs";
                };
                // **"A plain identifier" is not the same as "walkable", and the
                // first draft of this stopped here.** `base_symbol_of_heritage_entry`
                // resolves the name in `SymbolFlags::TYPE` and then requires
                // `CLASS | INTERFACE`; a base naming a type alias or an import
                // alias fails that test and makes the whole lookup a miss, and
                // the owner of *that* is the alias surface, not this file.
                let base_symbol = base_name.node_id.and_then(|id| {
                    bound.resolve_name(nodes, map, id, base_name.text, SymbolFlags::TYPE)
                });
                let Some(base_symbol) = base_symbol else {
                    return "base name does NOT RESOLVE in type meaning — resolution";
                };
                let flags = bound.symbols().get(base_symbol).flags;
                if flags.intersects(SymbolFlags::ALIAS) {
                    return "base resolves to an ALIAS — the alias surface, not members.rs";
                }
                if !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
                    return "base is neither class nor interface (type alias?) — declared.rs";
                }
            }
        }
    }
    // **The decisive test, and "walkable" alone is not it.**
    // `get_property_of_declared_symbol` already recurses into bases, so a
    // member sitting in a walkable base WOULD be found. Claiming these lines
    // for `members.rs` on the strength of the base being well-formed is the
    // same inference that produced "lookup rejected it" — an inference in the
    // direction that invents work for this workstream. So ask whether the
    // member is anywhere in the transitive base chain's binder tables at all.
    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
        return "not an access";
    };
    let Some(tsr_ast::MemberName::Identifier(member)) = node.name else {
        return "member name is not an identifier";
    };
    let mut queue = vec![symbol];
    let mut seen = Vec::new();
    while let Some(current) = queue.pop() {
        if seen.contains(&current) {
            continue;
        }
        seen.push(current);
        if bound.symbols().get(current).members.contains_key(member.text) {
            return "member IS in the base chain — the WALK is what fails (members.rs, MINE)";
        }
        let declarations = bound.symbols().get(current).declarations.clone();
        for declaration in declarations {
            let clauses = match map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
                _ => continue,
            };
            for clause in clauses {
                for base in clause.types {
                    if let Some(tsr_ast::Expression::Identifier(name)) = base.expression
                        && let Some(next) = name.node_id.and_then(|id| {
                            bound.resolve_name(nodes, map, id, name.text, SymbolFlags::TYPE)
                        })
                    {
                        queue.push(next);
                    }
                }
            }
        }
    }
    "member is NOWHERE in the base chain — not the walk; merge, alias or index signature"
}

/// The **this-container**, as upstream's `getThisContainer` finds it.
///
/// Ported for measurement only from `getThisContainer`
/// (`checker.go:12188`, from `grep -n` on the declaration) as it is used by
/// `checkThisExpression` (`checker.go:12077`): walk parents, and an
/// `ArrowFunction` is **transparent** — it does not own `this`, so the walk
/// continues through it. Every other function-like form is opaque.
///
/// The computed-property-name and decorator cases upstream handles are **not**
/// reproduced; they would land in the arm below them and are rare enough that
/// mixing them in is the smaller error than a partial port pretending to be a
/// whole one. Named so a reader does not assume they were handled.
// The `ArrowFunction` arm is deliberately identical to the wildcard: an arrow
// is transparent to `this`, which is upstream's **rule**
// (`checkThisExpression` skips arrows explicitly) and not an omission. Deleting
// the arm to satisfy the lint would delete the statement that it was
// considered — `docs/conventions.md`, "a limitation naming no owner is how
// stale comments outlive their truth".
#[allow(clippy::match_same_arms)]
fn this_container_label(nodes: &tsr_ast::NodeTable, node: NodeId) -> &'static str {
    let mut current = nodes.parent(node);
    while let Some(id) = current {
        match nodes.kind(id) {
            // Transparent, and this is a rule rather than an omission.
            SyntaxKind::ArrowFunction => {}
            SyntaxKind::MethodDeclaration => return "class/object METHOD",
            SyntaxKind::Constructor => return "CONSTRUCTOR",
            SyntaxKind::GetAccessor | SyntaxKind::SetAccessor => return "ACCESSOR",
            SyntaxKind::PropertyDeclaration => return "PROPERTY INITIALISER",
            SyntaxKind::FunctionDeclaration | SyntaxKind::FunctionExpression => {
                return "free FUNCTION — rebinds `this`";
            }
            SyntaxKind::ObjectLiteralExpression => return "OBJECT LITERAL",
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                return "class body, no nearer container";
            }
            SyntaxKind::ModuleDeclaration => return "MODULE/NAMESPACE body — upstream errors",
            SyntaxKind::SourceFile => return "SOURCE FILE — globalThis or undefined",
            _ => {}
        }
        current = nodes.parent(id);
    }
    "no container found"
}

/// Why a member lookup on a **correctly typed `this`** still failed.
///
/// These lines reached the `RIGHT` arm, so `this` itself was typed the way
/// upstream types it. The failure is therefore **downstream of `this` typing**,
/// and this asks which of the already-named items it is — which is the
/// difference between an `expressions.rs` spec and work in files this
/// workstream already owns.
fn this_receiver_verdict(
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    access: NodeId,
) -> &'static str {
    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
        return "not an access";
    };
    let Some(tsr_ast::MemberName::Identifier(name)) = node.name else {
        return "member name is not an identifier";
    };
    // The enclosing class, which is what `this` is typed as.
    let mut current = nodes.parent(access);
    let mut class = None;
    while let Some(id) = current {
        if matches!(nodes.kind(id), SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression) {
            class = Some(id);
            break;
        }
        current = nodes.parent(id);
    }
    let Some(class) = class else {
        return "no enclosing class — the `this` TYPING item (expressions.rs)";
    };
    let Some(symbol) = bound.symbol_of(class) else { return "the class has no symbol" };
    let entry = bound.symbols().get(symbol);
    if entry.members.contains_key(name.text) {
        // **The discriminator, and the first draft of this function did not
        // have it.** "The member is in the binder's table" does NOT mean the
        // lookup rejected it — the `RIGHT` arm holds every gap line whose
        // receiver is correctly typed, including lines where the member was
        // found and its own TYPE gapped. Labelling those "lookup rejected it"
        // would have been a false claim about `members.rs`, in the direction
        // that invents work for this workstream. So ask the lookup itself.
        let Some(receiver) = node.expression else { return "no receiver" };
        let receiver_type = checker.check_expression(receiver);
        return if checker.get_property_of_type(receiver_type, name.text).is_some() {
            "member FOUND by the lookup — its own TYPE gaps (the tsr-mcd family)"
        } else {
            "member in the binder's table, lookup MISSES it — members.rs, MINE"
        };
    }
    let has_base = entry.declarations.iter().any(|&declaration| match map.get(declaration) {
        Some(Node::ClassDeclaration(node)) => !node.heritage_clauses.is_empty(),
        Some(Node::ClassExpression(node)) => !node.heritage_clauses.is_empty(),
        _ => false,
    });
    if has_base {
        "inherited — the base-type walk (members.rs, MINE)"
    } else {
        "absent from the class and it has no base — a binder table or the static side"
    }
}

/// Is the receiver's named type an interface declared in **more than one
/// file**? — the cross-file merge split.
///
/// The test is on the printed type name looked up in `globals()`, which is the
/// table `merge_globals` writes into, and then on the **files** its
/// declarations live in. Two declarations in one file are an ordinary
/// same-file merge and are not this item; two in two files are.
///
/// **This is an approximation and its direction is stated rather than implied.**
/// It can only see receivers whose printed type is a bare global name, so it
/// **undercounts** — a receiver typed through a local alias or a namespace
/// member is missed entirely. It cannot overcount: a name with declarations in
/// two files really does have them.
fn merge_verdict(
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    access: NodeId,
) -> &'static str {
    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
        return "not an access";
    };
    let Some(receiver) = node.expression else { return "no receiver" };
    let receiver_type = checker.check_expression(receiver);
    let printed = checker.type_to_string(receiver_type);
    // **Resolve from the access site, in TYPE meaning, before falling back to
    // `globals()`.** Part 4 asked `globals()` only, which sees a script file's
    // top-level interface and nothing else — so every receiver typed through a
    // *module* file, a namespace or a local declaration fell into one
    // unmeasured bucket of 1,403. `resolve_name` is the same lookup the checker
    // itself used to reach the type, so it reaches the same symbol.
    let symbol = bound
        .resolve_name(nodes, map, access, printed.as_str(), SymbolFlags::TYPE)
        .or_else(|| bound.globals().get(printed.as_str()).copied());
    let Some(symbol) = symbol else {
        if printed.starts_with('{') {
            return "structural type — an object/type literal, no declaration to merge";
        }
        if printed.starts_with("typeof ") {
            return "typeof query — resolution, not members";
        }
        if printed.contains('.') {
            return "qualified name — a namespace member; resolution";
        }
        return "the receiver's type name does not resolve in TYPE meaning — resolution";
    };
    let files: std::collections::BTreeSet<NodeId> = bound
        .symbols()
        .get(symbol)
        .declarations
        .iter()
        .filter_map(|&declaration| {
            let mut current = Some(declaration);
            while let Some(id) = current {
                if nodes.kind(id) == SyntaxKind::SourceFile {
                    return Some(id);
                }
                current = nodes.parent(id);
            }
            None
        })
        .collect();
    if files.len() > 1 {
        return "MERGED ACROSS FILES — the getMergedSymbol item (binder)";
    }
    // One declaration site, and the member still was not found. The remaining
    // reasons are all about what `get_property_of_declared_symbol` can walk:
    // an unfollowable base (`extends B<T>`, `extends M.B`, `extends mixin()`)
    // makes the whole lookup a miss by design (`crate::members`). Separated
    // because that arm IS mine and the ones above are not.
    let has_base = bound.symbols().get(symbol).declarations.iter().any(|&declaration| {
        match map.get(declaration) {
            Some(Node::ClassDeclaration(node)) => !node.heritage_clauses.is_empty(),
            Some(Node::InterfaceDeclaration(node)) => !node.heritage_clauses.is_empty(),
            _ => false,
        }
    });
    if has_base {
        "declared once, HAS a heritage clause — base-type walk (members.rs, MINE)"
    } else {
        "declared once, no heritage clause — the member is genuinely absent"
    }
}

/// Which work item a failing receiver type belongs to.
///
/// Ordered, and the order is load-bearing: `"a" | "b"` is a union and
/// `Promise<A | B>` is a generic reference, so the bracket test must come before
/// the union test and the literal test before both.
///
/// This is a **syntactic test on a printed type**, which
/// `checker-notes-wrong.md` records as an upper bound on nothing in particular.
/// It is used here only to separate work items, never to size a conversion.
fn receiver_family(text: &str) -> &'static str {
    if matches!(text, "string" | "number" | "boolean" | "symbol" | "bigint" | "true" | "false") {
        "primitive           — needs getApparentType + a global interface (members.rs)"
    } else if text.starts_with('"') || text.starts_with('\'') || text.starts_with('`') {
        "string literal      — same apparent type as `string`"
    } else if text.chars().next().is_some_and(|c| c.is_ascii_digit())
        || (text.starts_with('-') && text.len() > 1)
    {
        "number literal      — same apparent type as `number`"
    } else if text.ends_with("[]") {
        "array               — apparent type is the generic `Array<T>` (bd tsr-4qx)"
    } else if text.contains('<') {
        "instantiated generic — bd tsr-4qx, NOT members.rs"
    } else if text == "this" {
        "this                — the `this` type"
    } else if text.contains(" | ") || text.contains(" & ") {
        "union / intersection — apparent type of each constituent"
    } else {
        "named / other       — an interface whose member we did not find"
    }
}

/// Whether `child` is the **member name** of `node` — the `b` of `a.b`.
///
/// Not an operand: `gap_reason` types the member name *as the access*
/// (`types_producer.rs`), so it carries the access's own failure back down. A
/// span descent that follows it walks straight back up through the
/// `member-of-access` edge, and the first run of this probe filed **20,232
/// lines (14.19%) as `CYCLE`** — every one of them that loop.
fn is_member_name_of(
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    node: NodeId,
    child: NodeId,
) -> bool {
    nodes.kind(node) == SyntaxKind::PropertyAccessExpression
        && nodes.parent(child) == Some(node)
        && map.get(node).and_then(|node| node.name_id()) == Some(child)
}

/// Whether a reason names a dependency this probe descends on. C1's input: a
/// node that stopped the walk while still naming one is a descent that failed
/// silently, and by construction there is no such node.
fn names_dependency(reason: &str) -> bool {
    reason.contains("the receiver is a gap")
        || reason.contains("/ initialiser ")
        || reason.starts_with("member name, ")
}

/// One step. The order mirrors `rank_board::cause` exactly — named receiver,
/// then span, then the sibling forms — so a disagreement between the two boards
/// is a disagreement about *depth*, never about the first step.
#[allow(clippy::too_many_arguments)]
fn descend(
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    node: NodeId,
    reason: &str,
    first_gapped: Option<usize>,
    has_inner: bool,
    line_ids: &[NodeId],
) -> Step {
    // 0. THE MEMBER NAME IS THE ACCESS. `gap_reason` routes `b` of `a.b`
    //    through `access_reason` on the parent — the two lines are one failure
    //    rendered twice. Not descending here would file 4,016 member-name lines
    //    under a leaf label while their access twins root elsewhere.
    if reason.starts_with("member name, ") {
        return nodes
            .parent(node)
            .filter(|&parent| nodes.kind(parent) == SyntaxKind::PropertyAccessExpression)
            .map_or_else(
                || Step::Stop(Root::BrokenDescent, format!("member name without access: {reason}")),
                |parent| Step::Go(parent, "member-of-access"),
            );
    }
    // 1. THE NAMED RECEIVER. `receiver_gap`'s edge.
    if reason.contains("the receiver is a gap") {
        let access = if nodes.kind(node) == SyntaxKind::PropertyAccessExpression {
            Some(node)
        } else {
            nodes.parent(node).filter(|&p| nodes.kind(p) == SyntaxKind::PropertyAccessExpression)
        };
        let receiver = match access.and_then(|access| map.get(access)) {
            Some(Node::PropertyAccessExpression(access)) => {
                access.expression.as_ref().and_then(tsr_ast::Expression::node_id)
            }
            _ => None,
        };
        return receiver.map_or_else(
            || Step::Stop(Root::BrokenDescent, format!("no receiver node: {}", row_key(reason))),
            |receiver| Step::Go(receiver, "receiver"),
        );
    }
    // 2. THE SPAN. Attribution is to the **outermost-leftmost** top-level gapped
    //    child; the branch histogram says how often that choice was forced.
    if let Some(first) = first_gapped {
        return Step::Go(line_ids[first], "span");
    }
    // 3. THE ANNOTATION and the alias right-hand side are type nodes and are
    //    where this instrument stops. Handled by `classify_stop`.
    // 4. THE INITIALISER, which `rank_board` could not follow at all.
    if reason.contains("/ initialiser ") {
        let symbol = symbol_of_reason(bound, nodes, map, node, reason);
        let initialiser = symbol
            .and_then(|symbol| bound.symbols().get(symbol).value_declaration)
            .and_then(|declaration| map.get(declaration))
            .and_then(|declaration| declaration.initializer_id());
        return initialiser.map_or_else(
            || Step::Stop(Root::BrokenDescent, format!("no initialiser node: {}", row_key(reason))),
            |initialiser| Step::Go(initialiser, "initialiser"),
        );
    }
    // 5. THE PROPERTY'S OWN DECLARATION — **RULE-2's B2**, and the arm the span
    //    test cannot stand in for. `property access, the property has no type`
    //    means `get_property_of_type` **found** the member and
    //    `get_type_of_symbol` answered `error` for it. The thing that gapped is
    //    the property's declaration, which is somewhere else in the program
    //    entirely, so nothing inside the access's span gapped and `gaproot`
    //    without this arm calls the access its own root. That is the same blind
    //    spot that let a 2,618-line row read TERMINAL and measure 68.6%
    //    propagated (`checker-notes-rank.md`, correction header).
    //
    //    Mirrors `access_reason` step for step — `check_expression` on the
    //    receiver, then `get_property_of_type` with the member name — so the
    //    symbol reached here is the symbol the reason was written about.
    if PROPERTY_DECLARATION_EDGE && reason.contains("the property has no type") {
        let Some(Node::PropertyAccessExpression(access)) = map.get(node) else {
            return Step::Stop(
                Root::BrokenDescent,
                format!("property-has-no-type on a non-access: {}", row_key(reason)),
            );
        };
        let receiver = access.expression;
        let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
            return Step::Stop(
                Root::BrokenDescent,
                format!("property name is not an identifier: {}", row_key(reason)),
            );
        };
        let property = receiver
            .map(|receiver| checker.check_expression(receiver))
            .and_then(|receiver_type| checker.get_property_of_type(receiver_type, name.text));
        let declaration_name = property
            .and_then(|property| bound.symbols().get(property).value_declaration)
            .and_then(|declaration| map.get(declaration))
            .and_then(|declaration| declaration.name_id());
        return declaration_name.map_or_else(
            || {
                Step::Stop(
                    Root::BrokenDescent,
                    format!("property with no named declaration: {}", row_key(reason)),
                )
            },
            |declaration_name| Step::Go(declaration_name, "property-declaration"),
        );
    }
    let (root, key) = classify_stop(reason, has_inner);
    Step::Stop(root, key)
}

/// The symbol `gap_reason` described, resolved the same way it resolved it.
///
/// Two paths, and taking the wrong one attributes lines to a declaration the
/// checker never saw: a *declaration name* is its parent's symbol, and a
/// *reference* is `resolve_name` in `SymbolFlags::VALUE` — which is what
/// `expressions.rs`'s identifier arm passes, and what `gap_reason` mirrors.
fn symbol_of_reason(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    node: NodeId,
    reason: &str,
) -> Option<tsr_binder::SymbolId> {
    if reason.starts_with("declaration name, ") {
        return nodes.parent(node).and_then(|parent| bound.symbol_of(parent));
    }
    match map.get(node) {
        Some(Node::Identifier(identifier)) => identifier
            .node_id
            .and_then(|id| bound.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)),
        _ => None,
    }
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

fn delta(left: usize, right: usize) -> i128 {
    i128::try_from(left).unwrap_or_default() - i128::try_from(right).unwrap_or_default()
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let cut: String = text.chars().take(width - 1).collect();
        format!("{cut}…")
    }
}

#[allow(clippy::too_many_lines)]
fn report(reports: &[CaseReport]) {
    let expected: usize = reports.iter().map(|r| r.expected).sum();
    let right: usize = reports.iter().map(|r| r.right).sum();
    let gap: usize = reports.iter().map(|r| r.gap).sum();
    let wrong: usize = reports.iter().map(|r| r.wrong).sum();

    println!("# gaproot — every gap line walked to the gap with no gap under it\n");
    println!("cases judged:              {}", reports.len());
    println!("upstream assertion lines:  {expected}");
    println!("  right:                   {right} ({:.2}%)", pct(right, expected));
    println!("  gap  (we said `error`):  {gap} ({:.2}%)", pct(gap, expected));
    println!("  wrong (ported, defect):  {wrong} ({:.2}%)", pct(wrong, expected));
    let aligned: usize = reports.iter().map(|r| r.aligned).sum();
    let unaligned: usize = reports.iter().map(|r| r.unaligned).sum();
    println!(
        "  unaligned:               {unaligned} — the walker did not reproduce upstream's \
         text; no type comparison is possible and no checker work touches it"
    );
    println!(
        "\nDENOMINATOR. `expected` counts UPSTREAM's baseline lines, which is what\n  \
         `types_suite::compare` counts (`assertion_count`), so every share above is over the\n  \
         suite's own population. Every share below is over the {gap} gap lines and says so.\n  \
         CONTROL right+gap+wrong-aligned = {} (must be 0)\n  \
         CONTROL aligned+unaligned-expected = {} (must be 0)",
        delta(right + gap + wrong, aligned),
        delta(aligned + unaligned, expected)
    );

    // Roll the roots up over the corpus.
    let mut rows: HashMap<(Root, String), Row> = HashMap::new();
    let mut by_kind: BTreeMap<Root, usize> = BTreeMap::new();
    for case in reports {
        for ((root, key), tally) in &case.roots {
            let row = rows.entry((*root, key.clone())).or_default();
            row.tally.merge(tally);
            *row.by_case.entry(case.name.clone()).or_default() += tally.lines;
            *by_kind.entry(*root).or_default() += tally.lines;
        }
    }
    let attributed: usize = by_kind.values().sum();

    println!("\n## WHERE THE WALK ENDS — unit: gap assertion lines, over {gap}\n");
    println!("{:<24} {:>9} {:>8}", "outcome", "lines", "share");
    let mut kinds: Vec<_> = by_kind.iter().collect();
    kinds.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (root, n) in kinds {
        println!("{:<24} {n:>9} {:>7.2}%", root.label(), pct(*n, gap));
    }

    println!("\n## THE BOARD'S ONE-STEP SPLIT, re-measured here (unit: gap lines)\n");
    println!("  Reconciles this probe's population against `rank_board`'s published split.");
    let mut board: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (key, n) in &case.board {
            *board.entry(key).or_default() += n;
        }
    }
    for (key, n) in &board {
        println!("  {n:>9} {:>7.2}%  {key}", pct(*n, gap));
    }
    // C7, and it is the only control here pinned by **another instrument**.
    // `rank_board` at `b5decc5` publishes these two totals over the same
    // population; this probe recomputes them along its own code path. Under a
    // polarity inversion of the span test they move by tens of thousands of
    // lines while every arithmetic control still reads zero — which is exactly
    // what mutation M1 measured, and what C1–C4 could not see, because C4's
    // board predicate is fed from the same span function the descent uses.
    // **The constants are the compiler's, not the probe's**, so they move
    // whenever the checker moves — including when this workstream moves it.
    // Their history in one session: 22,739 / 45,814 at `b5decc5`, 22,764 /
    // 44,342 at `b9a4f5c`, 22,793 / 44,254 after the `getApparentType` slice
    // landed, 22,354 / 43,250 at `5eb252c`, and 22,536 / 43,273 at `9f8bdba`.
    // **Five values in one session**, four of them from other workstreams'
    // merges and one from this workstream's own change. Each time, a stale constant read as a defect
    // in this probe and was not one.
    //
    // The honest reading after four: this control is a **tripwire, not an
    // invariant**. It cannot be left green across a moving `main`, and anyone
    // re-running this probe on a different commit must expect it non-zero and
    // re-take both numbers from `rank_board` in the same worktree before
    // concluding anything about the walk.
    //
    // That is the standing cost of a cross-instrument control, and it is paid
    // deliberately: M1 proved that C1-C4 cannot see a polarity inversion of the
    // span test, because C4's board predicate is fed by the same function the
    // descent uses, and C7 is the only control here that can. The commit is
    // named in the printed line rather than in a comment so a reader who sees
    // it non-zero checks the commit before checking the walk.
    // **C7 is two legs and only the first is an invariant.**
    //
    // `rank_board` builds its checker with `Checker::new`; this probe uses
    // `Checker::with_module_host`, which is what `render_case` itself builds
    // (ADR-0041) and is therefore the configuration the gradient is scored
    // through. The host can only ever *give a receiver a type*, and the only
    // arm that reads is `the receiver is a gap` — so it moves lines **out of**
    // `propagated/named` and into whatever they turn out to be.
    //
    // `TERMINAL` requires that no dependency is named at all, so **no line the
    // module host affects can enter or leave it**. That makes leg A an
    // invariant across the two instruments rather than a coincidence, and it is
    // the leg carrying the evidence. Measured at `5eb252c`: named −386, span
    // +193, UNMATCHED +292, DEPENDENT-UNKNOWN −99, netting to zero — the
    // divergence has grown from 54 lines at `b9a4f5c` as the cross-file seam
    // has gained answers, which is the direction it should grow in.
    let board_terminal = board.get("TERMINAL").copied().unwrap_or_default();
    let board_named = board.get("propagated/named").copied().unwrap_or_default();
    println!(
        "  C7a TERMINAL - rank_board@150b8ba's 22,538 = {} (0 AT THAT COMMIT ONLY. The \
         invariant is that the module host cannot move TERMINAL; the CONSTANT is the \
         compiler's and has taken six values today. Re-take it, do not debug the walk.)",
        delta(board_terminal, 22_538)
    );
    println!(
        "  C7b propagated/named - rank_board@150b8ba's 16,590 = {} (a MEASUREMENT: the \
         module-host divergence: 54 at b9a4f5c, then -386 at 5eb252c, 9f8bdba and 150b8ba - FIXED, not growing)",
        delta(board_named, 16_590)
    );
    println!(
        "  A4  this probe's five buckets - gap total = {} (arithmetic; the divergence \
         redistributes and never loses a line)",
        delta(board.values().sum::<usize>(), gap)
    );

    println!("\n## DEPTH HISTOGRAM — how far each gap line had to be walked (unit: gap lines)\n");
    let mut depth: BTreeMap<usize, usize> = BTreeMap::new();
    for case in reports {
        for (d, n) in &case.depth_hist {
            *depth.entry(*d).or_default() += n;
        }
    }
    for (d, n) in &depth {
        println!("  depth {d:>3}: {n:>9} ({:>6.2}%)", pct(*n, gap));
    }
    let deep: usize = depth.iter().filter(|(d, _)| **d > 0).map(|(_, n)| n).sum();
    println!(
        "  depth >0 : {deep:>9} ({:>6.2}%) — lines the board could not have attributed",
        pct(deep, gap)
    );

    println!("\n## BRANCHING — top-level gapped children at each span step (unit: span steps)\n");
    println!("  1 means the attribution was forced. >1 means this probe credits the FIRST");
    println!("  (outermost-leftmost) and does not credit the others — an undercount of");
    println!("  every root that is not first, and the size of it is this table.");
    let mut branch: BTreeMap<usize, usize> = BTreeMap::new();
    for case in reports {
        for (b, n) in &case.branch_hist {
            *branch.entry(*b).or_default() += n;
        }
    }
    let branch_total: usize = branch.values().sum();
    for (b, n) in branch.iter().take(8) {
        println!("  {b:>3} child(ren): {n:>9} ({:>6.2}%)", pct(*n, branch_total));
    }
    let many: usize = branch.iter().filter(|(b, _)| **b > 8).map(|(_, n)| n).sum();
    println!("   >8 children: {many:>9} ({:>6.2}%)", pct(many, branch_total));

    println!("\n## EDGES TAKEN (unit: descent steps, not lines)\n");
    let mut edges: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (edge, n) in &case.edges {
            *edges.entry(edge).or_default() += n;
        }
    }
    let mut edge_rows: Vec<_> = edges.iter().collect();
    edge_rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (edge, n) in edge_rows {
        println!("  {n:>9}  {edge}");
    }

    println!("\n## RANKING — ROOTS BY GAP LINES UNBLOCKED (unit: gap assertion lines)\n");
    println!("  The number beside a row is a CEILING: every line whose chain ends here.");
    println!("  `own` is how many of them ARE this row (depth 0); the rest are downstream.");
    println!("  `unnam` is the share of blocked lines whose baseline RHS is `typeof …` or");
    println!("  `import(…)` — forms this port has no route to naming. `any` is the share");
    println!("  upstream answers `any` on, closable only by widening.");
    let mut ranked: Vec<_> = rows.iter().collect();
    ranked
        .sort_by_key(|((root, key), row)| (std::cmp::Reverse(row.tally.lines), *root, key.clone()));
    println!(
        "{:<54} {:>8} {:>6} {:>8} {:>6} {:>6} {:>6} {:>6} {:>6}  kind",
        "root row", "lines", "share", "own", "cases", "top-1", "top-10", "unnam", "any"
    );
    for ((root, key), row) in ranked.iter().take(40) {
        let (cases, top1, top10) = row.concentration();
        println!(
            "{:<54} {:>8} {:>5.2}% {:>8} {:>6} {:>5.1}% {:>5.1}% {:>5.1}% {:>5.1}%  {}",
            truncate(key, 52),
            row.tally.lines,
            pct(row.tally.lines, gap),
            row.tally.own,
            cases,
            top1,
            top10,
            pct(row.tally.unnameable, row.tally.lines),
            pct(row.tally.any_rhs, row.tally.lines),
            root.label(),
        );
    }

    println!("\n## WHERE THE TOP ROOTS ACTUALLY LIVE (top three cases each)\n");
    for ((root, key), row) in ranked.iter().take(15) {
        let top: Vec<String> =
            row.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
        println!(
            "  {:<48} {:>8}  {:<20} {}",
            truncate(key, 46),
            row.tally.lines,
            root.label(),
            top.join(" | ")
        );
    }

    println!("\n## CAN THIS PORT SPELL IT? Baseline RHS for the lines each top root blocks\n");
    for ((root, key), row) in ranked.iter().take(12) {
        let mut rhs: Vec<_> = row.tally.rhs.iter().collect();
        rhs.sort_by_key(|(text, n)| (std::cmp::Reverse(**n), (*text).clone()));
        let sample: Vec<String> =
            rhs.iter().take(5).map(|(text, n)| format!("{} {n}", truncate(text, 22))).collect();
        println!("  {:<44} {}  [{}]", truncate(key, 42), root.label(), sample.join(" | "));
    }

    println!("\n## THE NAME `row_key` CUT — unresolved type references, by lines blocked\n");
    println!("  `annotation TypeReference unresolved` is one row above; this is what it is");
    println!("  made of. A lib name here is a lib item; a program name is a resolution item.");
    let mut names: HashMap<&str, usize> = HashMap::new();
    for case in reports {
        for (name, n) in &case.unresolved_names {
            *names.entry(name.as_str()).or_default() += n;
        }
    }
    let mut ranked_names: Vec<_> = names.into_iter().map(|(text, n)| (n, text)).collect();
    ranked_names.sort_unstable_by(|a, b| b.cmp(a));
    for (n, text) in ranked_names.iter().take(15) {
        println!("  {n:>7}  {text}");
    }

    println!("\n## IS THE GAP A LIST? Cumulative coverage by root row (unit: gap lines)\n");
    let mut cumulative = 0usize;
    println!("{:>6} {:>10} {:>8}", "rows", "lines", "share");
    for limit in [1usize, 3, 5, 10, 20, 30, 50, 100, 200, 500] {
        cumulative = ranked.iter().take(limit).map(|(_, row)| row.tally.lines).sum();
        println!("{limit:>6} {cumulative:>10} {:>7.2}%", pct(cumulative, gap));
    }
    let _ = cumulative;
    println!("  distinct root rows in total: {}", rows.len());

    println!("\n## RULE-2 / B1 — IS THE PREREQUISITE MET IN FACT? (unit: gap lines)\n");
    println!("  For every line whose ROOT is a property-access row: is the receiver of");
    println!("  that access typed the way upstream types it? A receiver typed WRONGLY does");
    println!("  not gap, so nothing inside the access's span gapped, so this probe calls");
    println!("  the access its own root — while the lookup failed on a type `members.rs`");
    println!("  was never given a chance to search. Those lines are not the row's work.");
    println!("  PROPERTY_DECLARATION_EDGE = {PROPERTY_DECLARATION_EDGE}");
    let mut fidelity: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for case in reports {
        for ((row, verdict), n) in &case.receiver_fidelity {
            *fidelity.entry((row.as_str(), verdict)).or_default() += n;
        }
    }
    let mut rows_seen: Vec<&str> = fidelity.keys().map(|(row, _)| *row).collect();
    rows_seen.sort_unstable();
    rows_seen.dedup();
    for row in rows_seen {
        let total: usize = fidelity.iter().filter(|((r, _), _)| *r == row).map(|(_, n)| *n).sum();
        println!("\n  {row}  —  {total} lines rooted here");
        for ((r, verdict), n) in &fidelity {
            if *r == row {
                println!("      {n:>8} {:>7.2}%  {verdict}", pct(*n, total));
            }
        }
    }
    // Which *kind* of receiver the lookup failed on, because the row is not one
    // work item: a primitive receiver needs `getApparentType` and a non-generic
    // global interface, and that is `members.rs`; an instantiated generic
    // receiver needs `bd tsr-4qx` and is not.
    println!("\n  what KIND of receiver the lookup failed on (unit: gap lines rooted here,");
    println!("  restricted to the ones whose receiver IS typed as upstream types it):");
    let mut families: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (text, n) in &case.receiver_types {
            *families.entry(receiver_family(text)).or_default() += n;
        }
    }
    let family_total: usize = families.values().sum();
    let mut family_rows: Vec<_> = families.into_iter().map(|(f, n)| (n, f)).collect();
    family_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, family) in family_rows {
        println!("      {n:>8} {:>7.2}%  {family}", pct(n, family_total));
    }

    println!("\n  the receiver types the lookup failed on, where the receiver IS correct:");
    let mut receiver_types: HashMap<&str, usize> = HashMap::new();
    for case in reports {
        for (text, n) in &case.receiver_types {
            *receiver_types.entry(text.as_str()).or_default() += n;
        }
    }
    let mut ranked_types: Vec<_> = receiver_types.into_iter().map(|(t, n)| (n, t)).collect();
    ranked_types.sort_unstable_by(|a, b| b.cmp(a));
    for (n, text) in ranked_types.iter().take(15) {
        println!("      {n:>8}  {}", truncate(text, 60));
    }

    println!("\n  THE PRIMITIVE SLICE — what upstream prints for the lines it would unblock:");
    let mut primitive_rhs: HashMap<&str, usize> = HashMap::new();
    let mut primitive_cases: HashMap<&str, usize> = HashMap::new();
    for case in reports {
        for (text, n) in &case.primitive_rhs {
            *primitive_rhs.entry(text.as_str()).or_default() += n;
        }
        for (name, n) in &case.primitive_by_case {
            *primitive_cases.entry(name.as_str()).or_default() += n;
        }
    }
    let primitive_total: usize = primitive_rhs.values().sum();
    let signatures: usize =
        primitive_rhs.iter().filter(|(text, _)| text.contains("=>")).map(|(_, n)| *n).sum();
    let mut primitive_ranked: Vec<_> = primitive_rhs.into_iter().map(|(t, n)| (n, t)).collect();
    primitive_ranked.sort_unstable_by(|a, b| b.cmp(a));
    for (n, text) in primitive_ranked.iter().take(12) {
        println!("      {n:>8}  {}", truncate(text, 62));
    }
    println!(
        "      of {primitive_total} lines, {signatures} ({:.1}%) print a CALL SIGNATURE — a \
         method, not a property",
        pct(signatures, primitive_total)
    );
    let mut primitive_case_ranked: Vec<_> =
        primitive_cases.into_iter().map(|(n, c)| (c, n)).collect();
    primitive_case_ranked.sort_unstable_by(|a, b| b.cmp(a));
    let top1 = primitive_case_ranked.first().map_or(0, |(n, _)| *n);
    let top10: usize = primitive_case_ranked.iter().take(10).map(|(n, _)| n).sum();
    println!(
        "      concentration: {} cases, top-1 {:.1}%, top-10 {:.1}%  [{}]",
        primitive_case_ranked.len(),
        pct(top1, primitive_total),
        pct(top10, primitive_total),
        primitive_case_ranked
            .iter()
            .take(3)
            .map(|(n, c)| format!("{c} {n}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );

    println!("\n## RULE-3 / P1 — the `PropertyAccessExpression` POPULATION (unit: gap lines)\n");
    println!("  |P| is pinned by the AST: a node's kind cannot move under the checker.");
    println!("  Arms mirror checker-notes-evolvearray.md's ElementAccess split one for one.");
    println!("  `RIGHT` carries the positive test; `not rendered` is the default arm.");
    let mut arms: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (arm, n) in &case.access_arms {
            *arms.entry(arm).or_default() += n;
        }
    }
    let arms_total: usize = arms.values().sum();
    let mut arm_rows: Vec<_> = arms.iter().map(|(a, n)| (*n, *a)).collect();
    arm_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, arm) in arm_rows {
        println!("  {n:>8} {:>7.2}%  {arm}", pct(n, arms_total));
    }
    println!(
        "  {arms_total:>8}           TOTAL  (checker-notes-wrong.md's addendum reads 15,215 \
         gap for this kind)"
    );
    let mut right_cases: HashMap<&str, usize> = HashMap::new();
    let mut right_rhs: HashMap<&str, usize> = HashMap::new();
    let mut right_family: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (name, n) in &case.access_right_by_case {
            *right_cases.entry(name.as_str()).or_default() += n;
        }
        for (text, n) in &case.access_right_rhs {
            *right_rhs.entry(text.as_str()).or_default() += n;
        }
        for (family, n) in &case.access_right_family {
            *right_family.entry(family).or_default() += n;
        }
    }
    let right_total: usize = right_cases.values().sum();
    let mut right_ranked: Vec<_> = right_cases.into_iter().map(|(c, n)| (n, c)).collect();
    right_ranked.sort_unstable_by(|a, b| b.cmp(a));
    let top1 = right_ranked.first().map_or(0, |(n, _)| *n);
    let top10: usize = right_ranked.iter().take(10).map(|(n, _)| n).sum();
    println!(
        "\n  P1b concentration of RIGHT: {} cases, top-1 {:.1}%, top-10 {:.1}%  [{}]",
        right_ranked.len(),
        pct(top1, right_total),
        pct(top10, right_total),
        right_ranked
            .iter()
            .take(3)
            .map(|(n, c)| format!("{c} {n}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let any_rhs: usize = right_rhs.iter().filter(|(t, _)| **t == "any").map(|(_, n)| *n).sum();
    let unnameable: usize = right_rhs
        .iter()
        .filter(|(t, _)| t.starts_with("typeof ") || t.contains("import("))
        .map(|(_, n)| *n)
        .sum();
    println!(
        "  P1c spellability of RIGHT: `any` {any_rhs} ({:.1}%), unnameable {unnameable} ({:.1}%)",
        pct(any_rhs, right_total),
        pct(unnameable, right_total)
    );
    let mut rhs_ranked: Vec<_> = right_rhs.into_iter().map(|(t, n)| (n, t)).collect();
    rhs_ranked.sort_unstable_by(|a, b| b.cmp(a));
    println!("  what upstream prints for the RIGHT arm:");
    for (n, text) in rhs_ranked.iter().take(8) {
        println!("      {n:>7}  {}", truncate(text, 56));
    }
    println!("\n  RULE-3 / P2 — the RIGHT arm by receiver kind, which names the owner:");
    let mut family_ranked: Vec<_> = right_family.into_iter().map(|(f, n)| (n, f)).collect();
    family_ranked.sort_unstable_by(|a, b| b.cmp(a));
    for (n, family) in family_ranked {
        println!("      {n:>7} {:>7.2}%  {family}", pct(n, right_total));
    }

    println!("\n  RULE-4 — WHY THE MEMBER WAS NOT FOUND, over ALL RIGHT-arm lines (gap lines):");
    let mut merge: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (verdict, n) in &case.merge_split {
            *merge.entry(verdict).or_default() += n;
        }
    }
    let merge_total: usize = merge.values().sum();
    let mut merge_rows: Vec<_> = merge.iter().map(|(v, n)| (*n, *v)).collect();
    merge_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, verdict) in merge_rows {
        println!("      {n:>7} {:>7.2}%  {verdict}", pct(n, merge_total));
    }
    println!("\n  the same, crossed with the receiver kind:");
    let mut crossed: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for case in reports {
        for ((family, verdict), n) in &case.merge_by_family {
            *crossed.entry((family, verdict)).or_default() += n;
        }
    }
    let mut crossed_rows: Vec<_> = crossed.iter().map(|((f, v), n)| (*n, *f, *v)).collect();
    crossed_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, family, verdict) in crossed_rows.iter().take(14) {
        println!("      {n:>7}  {:<38}  {}", truncate(family, 36), truncate(verdict, 58));
    }

    println!(
        "\n## THE `this` SPEC — population A: gap lines whose NODE is `this` (unit: gap lines)\n"
    );
    println!("  |P| pinned by node kind. Container found by a measurement-only port of");
    println!("  upstream's getThisContainer (checker.go:12188): arrows are transparent.");
    let mut this_pop: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (container, n) in &case.this_population {
            *this_pop.entry(container).or_default() += n;
        }
    }
    let this_total: usize = this_pop.values().sum();
    let mut this_rows: Vec<_> = this_pop.iter().map(|(c, n)| (*n, *c)).collect();
    this_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, container) in &this_rows {
        println!("  {n:>7} {:>7.2}%  {container}", pct(*n, this_total));
    }
    println!("  {this_total:>7}           TOTAL");
    println!("\n  SPELLABILITY, per container — what upstream prints (the exact-match leg):");
    let mut rhs: HashMap<(&str, &str), usize> = HashMap::new();
    for case in reports {
        for ((container, text), n) in &case.this_rhs {
            *rhs.entry((container, text.as_str())).or_default() += n;
        }
    }
    for (_, container) in this_rows.iter().take(5) {
        let mut per: Vec<_> =
            rhs.iter().filter(|((c, _), _)| c == container).map(|((_, t), n)| (*n, *t)).collect();
        per.sort_unstable_by(|a, b| b.cmp(a));
        let total: usize = per.iter().map(|(n, _)| n).sum();
        let sample: Vec<String> =
            per.iter().take(4).map(|(n, t)| format!("{} {n}", truncate(t, 24))).collect();
        println!("    {:<44} {total:>6}  [{}]", truncate(container, 42), sample.join(" | "));
    }

    println!("\n## THE `this` SPEC — population B: `this` RECEIVERS in the RIGHT arm\n");
    println!("  These were typed the way upstream types them, so the failure is DOWNSTREAM");
    println!("  of `this` typing. This is what it actually is:");
    let mut this_recv: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (verdict, n) in &case.this_receiver {
            *this_recv.entry(verdict).or_default() += n;
        }
    }
    let recv_total: usize = this_recv.values().sum();
    let mut recv_rows: Vec<_> = this_recv.iter().map(|(v, n)| (*n, *v)).collect();
    recv_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, verdict) in recv_rows {
        println!("  {n:>7} {:>7.2}%  {verdict}", pct(n, recv_total));
    }

    println!("\n## RULE-5 — WHAT IS LEFT IN `members.rs` (unit: gap assertion lines)\n");
    for (label, map_of) in [
        ("tsr-n22, the 545: declared once, no heritage, member absent", 0usize),
        ("the base-type walk: declared once, HAS a heritage clause", 1),
        ("the 174: the member name is not an identifier", 2),
    ] {
        let mut rolled: BTreeMap<&str, usize> = BTreeMap::new();
        for case in reports {
            let source = match map_of {
                0 => &case.left_n22,
                1 => &case.left_base,
                _ => &case.left_name,
            };
            for (verdict, n) in source {
                *rolled.entry(verdict).or_default() += n;
            }
        }
        let total: usize = rolled.values().sum();
        println!("\n  {label} — {total} lines");
        let mut rows: Vec<_> = rolled.iter().map(|(v, n)| (*n, *v)).collect();
        rows.sort_unstable_by(|a, b| b.cmp(a));
        for (n, verdict) in rows {
            println!("      {n:>7} {:>7.2}%  {verdict}", pct(n, total));
        }
    }

    println!("\n## RULE-6 — THE LARGEST FAMILY, ROOT-SPLIT (unit: gap assertion lines)\n");
    let mut to_family: BTreeMap<(&str, bool), usize> = BTreeMap::new();
    let mut root_kind: BTreeMap<(&str, bool), usize> = BTreeMap::new();
    for case in reports {
        for (key, n) in &case.expr_to_root_family {
            *to_family.entry(*key).or_default() += n;
        }
        for (key, n) in &case.expr_root_kind {
            *root_kind.entry(*key).or_default() += n;
        }
    }
    let family_all: usize = to_family.values().sum();
    let ts2563: usize = to_family.iter().filter(|((_, t), _)| *t).map(|(_, n)| *n).sum();
    let reachable = family_all - ts2563;
    println!("  |P| = {family_all} lines in the family (rank_board's roll-up reads 51,784)");
    println!(
        "  TS2563 EXCLUDED = {ts2563} ({:.2}%) — upstream disabled control flow and answers",
        pct(ts2563, family_all)
    );
    println!("      `any`; unreachable by any checker work. Printed, never netted away.");
    println!("  REACHABLE = {reachable}\n");
    let mut ts_cases: HashMap<&str, usize> = HashMap::new();
    for case in reports {
        for (name, n) in &case.expr_ts2563_by_case {
            *ts_cases.entry(name.as_str()).or_default() += n;
        }
    }
    let mut ts_ranked: Vec<_> = ts_cases.into_iter().map(|(c, n)| (n, c)).collect();
    ts_ranked.sort_unstable_by(|a, b| b.cmp(a));
    let ts_top: Vec<String> = ts_ranked.iter().take(3).map(|(n, c)| format!("{c} {n}")).collect();
    println!("  the TS2563 lines live in {} case(s): {}", ts_ranked.len(), ts_top.join(" | "));

    println!("\n  WHERE THE FAMILY'S LINES ROOT — the transition matrix, TS2563 excluded:");
    let mut rows: Vec<_> =
        to_family.iter().filter(|((_, t), _)| !*t).map(|((f, _), n)| (*n, *f)).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    let mut outside = 0usize;
    for (n, destination) in &rows {
        if !destination.starts_with("AN EXPRESSION") {
            outside += n;
        }
        println!("    {n:>8} {:>7.2}%  {destination}", pct(*n, reachable));
    }
    println!(
        "\n    ROOTS OUTSIDE THE FAMILY: {outside} ({:.2}%) — RULE-6's P-a predicted >= 55%",
        pct(outside, reachable)
    );

    println!("\n  BY ROOT KIND within the family, TS2563 excluded:");
    let kind_total: usize = root_kind.iter().filter(|((_, t), _)| !*t).map(|(_, n)| *n).sum();
    let mut kind_rows: Vec<_> =
        root_kind.iter().filter(|((_, t), _)| !*t).map(|((k, _), n)| (*n, *k)).collect();
    kind_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, kind) in kind_rows {
        println!("    {n:>8} {:>7.2}%  {kind}", pct(n, kind_total));
    }

    println!("\n  WHAT THE EXPRESSION ACTUALLY IS — top forms, own root vs propagated:");
    let mut by_kind: HashMap<String, (usize, usize)> = HashMap::new();
    for case in reports {
        for ((kind, which), n) in &case.expr_by_kind {
            let entry = by_kind.entry(kind.clone()).or_default();
            if *which == "its own root" {
                entry.0 += n;
            } else {
                entry.1 += n;
            }
        }
    }
    let mut kinds: Vec<_> =
        by_kind.into_iter().map(|(k, (own, prop))| (own + prop, own, prop, k)).collect();
    kinds.sort_unstable_by(|a, b| b.cmp(a));
    println!(
        "    {:<34} {:>8} {:>9} {:>11} {:>8}",
        "form", "lines", "own root", "propagated", "own %"
    );
    for (total, own, prop, kind) in kinds.iter().take(16) {
        println!(
            "    {:<34} {total:>8} {own:>9} {prop:>11} {:>7.1}%",
            truncate(kind, 32),
            pct(*own, *total)
        );
    }

    println!("\n## RULE-7 — SPELLABILITY OVER THE SURVIVORS (unit: gap assertion lines)\n");
    println!("  Exact match on the baseline's right-hand side, not shape.");
    let mut e7: HashMap<(&str, bool, bool, &str), usize> = HashMap::new();
    for case in reports {
        for ((kind, own, ts, text), n) in &case.e7_rhs {
            *e7.entry((kind, *own, *ts, text.as_str())).or_default() += n;
        }
    }
    // A raw roll-up printed first, because two populations reported an
    // identical propagated count on the first run and an unexplained
    // coincidence is exactly the thing this project says to chase rather than
    // publish. This table is keyed straight off the map with no filtering.
    let mut raw: BTreeMap<(&str, bool, bool), usize> = BTreeMap::new();
    for case in reports {
        for ((kind, own, ts), n) in &case.e7_totals {
            *raw.entry((*kind, *own, *ts)).or_default() += n;
        }
    }
    let dropped: usize = reports.iter().map(|r| r.e7_dropped).sum();
    println!("  RAW ROLL-UP (UNCAPPED)  (kind, own-root, ts2563) -> lines");
    for ((kind, own, ts), n) in &raw {
        println!("    {:<26} own={own:<5} ts2563={ts:<5} {n:>7}", truncate(kind, 24));
    }
    println!(
        "    histogram increments dropped by the per-case cap: {dropped} \
         (the TOTALS above are uncapped; only the type histograms below are)"
    );
    for kind in ["ElementAccessExpression", "ObjectLiteralExpression"] {
        println!("\n  === {kind}");
        // The assumption the 88.4% rested on, measured rather than assumed.
        let ts_total: usize = raw.get(&(kind, true, true)).copied().unwrap_or_default()
            + raw.get(&(kind, false, true)).copied().unwrap_or_default();
        let ts_any: usize = e7
            .iter()
            .filter(|((k, _, t, text), _)| *k == kind && *t && *text == "any")
            .map(|(_, n)| *n)
            .sum();
        println!(
            "    TS2563 lines: {ts_total}, of which `any` {ts_any} ({:.1}%) — the assumption \
             the 88.4% rested on, measured",
            pct(ts_any, ts_total)
        );
        for (own, label) in [(true, "OWN ROOT (the candidate)"), (false, "propagated")] {
            let rows: Vec<(usize, &str)> = e7
                .iter()
                .filter(|((k, o, t, _), _)| *k == kind && *o == own && !*t)
                .map(|((_, _, _, text), n)| (*n, *text))
                .collect();
            // **From the uncapped map**, so a truncated histogram cannot
            // understate the denominator the percentages are taken over.
            let total = raw.get(&(kind, own, false)).copied().unwrap_or_default();
            let any: usize = rows.iter().filter(|(_, t)| *t == "any").map(|(n, _)| n).sum();
            let unnameable: usize = rows
                .iter()
                .filter(|(_, t)| t.starts_with("typeof ") || t.contains("import("))
                .map(|(n, _)| n)
                .sum();
            println!(
                "    {label}, TS2563 excluded: {total} lines | `any` {any} ({:.1}%) | \
                 unnameable {unnameable} ({:.1}%)",
                pct(any, total),
                pct(unnameable, total)
            );
            if own {
                let mut ranked = rows;
                ranked.sort_unstable_by(|a, b| b.cmp(a));
                let sample: Vec<String> = ranked
                    .iter()
                    .take(8)
                    .map(|(n, t)| format!("{} {n}", truncate(t, 20)))
                    .collect();
                println!("        {}", sample.join(" | "));
                let mut cases: HashMap<&str, usize> = HashMap::new();
                for case in reports {
                    for ((k, name), n) in &case.e7_by_case {
                        if *k == kind {
                            *cases.entry(name.as_str()).or_default() += n;
                        }
                    }
                }
                let case_total: usize = cases.values().sum();
                let mut ranked_cases: Vec<_> = cases.into_iter().map(|(c, n)| (n, c)).collect();
                ranked_cases.sort_unstable_by(|a, b| b.cmp(a));
                let top1 = ranked_cases.first().map_or(0, |(n, _)| *n);
                println!(
                    "        concentration: {} cases, top-1 {:.1}%  [{}]",
                    ranked_cases.len(),
                    pct(top1, case_total),
                    ranked_cases
                        .iter()
                        .take(3)
                        .map(|(n, c)| format!("{c} {n}"))
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
        }
    }

    println!("\n  E2 — WHICH MEMBER FORM MAKES `check_object_literal` BAIL (the 575):");
    let mut om: BTreeMap<&str, usize> = BTreeMap::new();
    for case in reports {
        for (verdict, n) in &case.e7_object_member {
            *om.entry(verdict).or_default() += n;
        }
    }
    let om_total: usize = om.values().sum();
    let mut om_rows: Vec<_> = om.iter().map(|(v, n)| (*n, *v)).collect();
    om_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (n, verdict) in om_rows {
        println!("      {n:>6} {:>7.2}%  {verdict}", pct(n, om_total));
    }

    println!("\n  E1 RE-MEASURED OVER THE SPREAD SUBSET ALONE (bd tsr-sps's precondition):");
    let mut srhs: HashMap<&str, usize> = HashMap::new();
    let mut scases: HashMap<&str, usize> = HashMap::new();
    for case in reports {
        for (text, n) in &case.e7_spread_rhs {
            *srhs.entry(text.as_str()).or_default() += n;
        }
        for (name, n) in &case.e7_spread_by_case {
            *scases.entry(name.as_str()).or_default() += n;
        }
    }
    let stotal: usize = srhs.values().sum();
    let sany: usize = srhs.iter().filter(|(t, _)| **t == "any").map(|(_, n)| *n).sum();
    let sunn: usize = srhs
        .iter()
        .filter(|(t, _)| t.starts_with("typeof ") || t.contains("import("))
        .map(|(_, n)| *n)
        .sum();
    println!(
        "      {stotal} lines | `any` {sany} ({:.1}%) | unnameable {sunn} ({:.1}%)  — bar is 25% each",
        pct(sany, stotal),
        pct(sunn, stotal)
    );
    let mut sranked: Vec<_> = srhs.into_iter().map(|(t, n)| (n, t)).collect();
    sranked.sort_unstable_by(|a, b| b.cmp(a));
    let sample: Vec<String> =
        sranked.iter().take(8).map(|(n, t)| format!("{} {n}", truncate(t, 22))).collect();
    println!("      {}", sample.join(" | "));
    let mut sc: Vec<_> = scases.into_iter().map(|(c, n)| (n, c)).collect();
    sc.sort_unstable_by(|a, b| b.cmp(a));
    let stop1 = sc.first().map_or(0, |(n, _)| *n);
    println!(
        "      concentration: {} cases, top-1 {:.1}%  [{}]",
        sc.len(),
        pct(stop1, stotal),
        sc.iter().take(3).map(|(n, c)| format!("{c} {n}")).collect::<Vec<_>>().join(" | ")
    );

    println!("\n## CONTROLS — what pins each\n");
    let c1: usize = reports.iter().map(|r| r.c1_root_names_dependency).sum();
    let c1m: usize = reports.iter().map(|r| r.c1_root_names_none).sum();
    let c2: usize = reports.iter().map(|r| r.c2_own_rule_on_leaf).sum();
    let c2m: usize = reports.iter().map(|r| r.c2_own_rule_has_inner).sum();
    let c3: usize = reports.iter().map(|r| r.c3_member_without_access).sum();
    let c3m: usize = reports.iter().map(|r| r.c3_member_with_access).sum();
    let c4: usize = reports.iter().map(|r| r.c4_board_terminal_deep).sum();
    let c4m: usize = reports.iter().map(|r| r.c4_board_terminal_depth0).sum();
    let c5: usize = reports.iter().map(|r| r.c5_steps_without_span_data).sum();
    println!(
        "  C1  a ROOT whose own reason still names a dependency   = {c1} (must be 0, by \
         CONSTRUCTION: every such reason has a descent arm)"
    );
    println!("  C1' a ROOT naming none (the mirror)                    = {c1m}");
    println!(
        "  C2  ROOT/own-rule on a node with nothing inside it     = {c2} (must be 0, by \
         CONSTRUCTION: `has_inner` is the arm's positive test)"
    );
    println!("  C2' ROOT/own-rule with something inside it (mirror)    = {c2m}");
    println!(
        "  C3  `member name,` line whose parent is not an access  = {c3} (must be 0, by \
         CONSTRUCTION: `gap_reason` emits the prefix only from that parent)"
    );
    println!("  C3' `member name,` line whose parent is an access      = {c3m}");
    println!(
        "  C4  a line the BOARD calls TERMINAL that walks deeper   = {c4} (must be 0, by \
         CONSTRUCTION: identical predicates, so the walk cannot move)"
    );
    println!("  C4' board-TERMINAL lines stopping at depth 0 (mirror)   = {c4m}");
    println!(
        "  C5  descent steps landing on an unrendered node        = {c5} (a measurement, \
         not a zero: no span evidence there, the reason alone decided)"
    );
    let skipped: usize = reports.iter().map(|r| r.span_skipped_member_name).sum();
    println!(
        "  C6  span steps that skipped an access's own member name = {skipped} (a \
         population, not a violation: each one is a `member-of-access` loop not entered)"
    );
    println!(
        "  A1  roots attributed - gap total                       = {} (arithmetic; \
         blind to anything that moves lines between buckets)",
        delta(attributed, gap)
    );
}
