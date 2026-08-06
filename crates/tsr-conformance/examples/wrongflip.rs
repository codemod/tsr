//! The **wrong** bucket, ranked by whether closing a row flips a case.
//!
//! `cargo run --release -p tsr-conformance --example wrongflip`
//!
//! # What this adds over `examples/rank_board.rs`
//!
//! `rank_board` ranks the **gap** (139,612 lines) properly: every gap row
//! carries a cause split, a concentration check and the level-4 `finishes`
//! statistic. Its treatment of the **wrong** bucket (37,489 lines) is two
//! histograms — node kind, and node kind × shape substitution — with no cause
//! split at all, because the TERMINAL/PROPAGATED machinery was written against
//! `gap_reason`, which only fires on lines that answered `error`.
//!
//! That omission matters more for wrong lines than for gap lines. A gap
//! propagates as a gap and is visibly a gap all the way up. A **wrong answer
//! propagates as a different wrong answer**: `var x = f(); x.p;` puts one wrong
//! line on `f()`, one on `x`, one on `x.p`, and all three land in different
//! rows of `rank_board`'s histogram, each looking like its own work item. So
//! this probe splits every wrong line by whether something it depends on also
//! failed, and reports `ROOT` and propagated lines apart.
//!
//! # The two propagation tests, and why one is not enough
//!
//! - **By descendant (span).** The walker emits in source preorder, so a node's
//!   descendants are the contiguous run of later lines whose span lies inside
//!   its own. If any of them failed, this node's inputs were not all correct.
//!   Copied in shape from `rank_board`, with **proper** containment required so
//!   the leaf control below can fire.
//! - **By declaration (name).** The span test is blind on a leaf, and 82.87% of
//!   the wrong bucket is `Identifier` — a leaf whose dependency is not inside
//!   it but somewhere else in the file. `docs/conventions.md` records this
//!   exact trap costing a row's entire ranking. So for an identifier the probe
//!   resolves the symbol and asks whether the **declaration's own rendered
//!   line** failed.
//!
//! `ROOT` — the loaded label, the one that means *"the row's size is its
//! worth"* — is a **positive** test, never the default arm: either the node is
//! a literal (nothing it can depend on, by the grammar), or its inputs are all
//! present and correct. `UNKNOWN` is the default, and it is real and non-empty:
//! an identifier that does not resolve, or one whose declaration the walker
//! never rendered. `docs/conventions.md`: *"never let the semantically loaded
//! label be the default arm."*
//!
//! # Controls, printed unconditionally
//!
//! Four of the six are pinned by **construction** rather than by arithmetic,
//! which is what lets them see a semantic inversion that leaves every sum
//! intact (`docs/conventions.md`). Each is printed with its mirror where a
//! mirror exists, so the pair pins the classifier rather than one arm of it.
//!
//! - **C1 — a leaf cannot contain anything.** For a node kind that is a leaf by
//!   the grammar (an identifier, a numeric/string/bigint/regex literal, a
//!   keyword), the count classified `propagated/descendant` is **0**, because
//!   proper containment inside a leaf is impossible. Fixed by the subject, true
//!   before a line of this file was written. Its mirror — non-leaf nodes so
//!   classified — is printed beside it, so an inversion that zeroes C1 by
//!   zeroing the whole test is visible.
//! - **C2 — a declaration is not its own dependency.** The count of lines
//!   classified `propagated/declaration` whose blamed declaration is the line's
//!   own is **0**, re-tested at the point of the verdict rather than inferred
//!   from the `continue` that enforces it — so deleting that `continue` makes
//!   it fire. Two mirrors are printed beside it: how often the exclusion
//!   *fired* (a population, not a violation) and how often another declaration
//!   was inspected. Both must be large, or the arm is unreachable and its zero
//!   means nothing.
//! - **C3 — an unaligned case cannot be finished by a checker row.** An
//!   unaligned line is one the walker did not reproduce the *text* of; it is in
//!   the residual and it is not a wrong line, so no wrong row can take it to
//!   zero. The count of (row, case) pairs scored `residual_after == 0` in a
//!   case that has at least one unaligned line is **0 by construction**. This
//!   is the one that catches a residual computed off the wrong denominator.
//! - **C6 — the residual agrees with another instrument.** Cases with residual
//!   0 must be the gate's passing cases. The gate reports **2,173** at
//!   `058b4a9`, and `checker-notes-rank.md` §7 records this residual metric
//!   reading a known **+18** against it, because it cannot see lines *we* emit
//!   that upstream does not. So the expected value is 2,173 + a small positive
//!   number; a negative difference, or a large one, is a defect **here**. This
//!   is the strongest kind available (`docs/conventions.md`): not a bucket
//!   reading zero, but two instruments that could disagree and do not.
//!
//! A fifth control was written and **deleted before the first run**: *"a wrong
//! line whose answer is `error` = 0"*. It sits after the branch that sends
//! every `error` answer to the gap bucket and `continue`s, so no input can make
//! it non-zero. That is precisely the failure `docs/conventions.md` records —
//! *"a control bucket over a classifier whose last arm is a default cannot
//! fire"* — and it is recorded here rather than silently removed, because the
//! next reader is otherwise free to reinvent it.
//! - **A1, A2 — arithmetic.** `right + gap + wrong == aligned`, and each
//!   ranking's row sum `== wrong total`. These catch a lost or double-counted
//!   line and nothing else.
//!
//! **The naming classifier below is total and is therefore NOT a control.** It
//! is said here rather than left for a reader to discover, because a total
//! classifier with a zero bucket beside it reads exactly like a proven
//! partition and proves nothing (`docs/conventions.md`).
//!
//! # Can this port spell the answer?
//!
//! For a wrong row the question sharpens: is the type wrong, or is the type
//! right and unnameable? Every top row therefore prints its exact
//! `upstream -> ours` pairs, and a naming split whose two nominal/structural
//! arms are the numerator of rule R3 in
//! `docs/architecture/checker-notes-wrong.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rayon::prelude::*;
use tsr_ast::{NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, type_shape, types_baseline, types_producer};

/// Why a wrong line is wrong *here* rather than somewhere else.
///
/// `Root` is the loaded label and carries a positive test. `Unknown` is the
/// default arm and is deliberately the least informative one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Prop {
    /// A line properly inside this node's span also failed. The row's size is
    /// an upper bound of unknown depth.
    Descendant,
    /// This is an identifier, and the rendered line for its declaration's name
    /// also failed. The leaf case the span test cannot see.
    Declaration,
    /// This is a **declaration name**, and a line inside the declaration it
    /// names — its annotation or its initialiser — also failed. The second leaf
    /// case the span test cannot see: `x` in `var x = f()` spans only `x`,
    /// while the thing it waits on is `f()` beside it.
    Sibling,
    /// Nothing this line depends on is known to have failed: either it is a
    /// literal (by the grammar it depends on nothing), or every rendered line
    /// inside it is right and, if it is a name, its declaration is right too.
    Root,
    /// No evidence either way. An identifier that does not resolve, or whose
    /// declaration the walker never rendered. Not evidence of `Root`.
    Unknown,
}

impl Prop {
    const fn label(self) -> &'static str {
        match self {
            Self::Descendant => "propagated/descendant",
            Self::Declaration => "propagated/declaration",
            Self::Sibling => "propagated/sibling",
            Self::Root => "ROOT",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Wrong type, or right type we cannot name.
///
/// **Total, therefore not a control.** Ordered; the order is load-bearing,
/// because a line can satisfy more than one test and the earlier arms are the
/// ones that say *"this is not a checker item"*.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Naming {
    /// `error` reached a *printed* type (`error[]`). A gap that escaped and was
    /// then scored as a wrong claim — neither a gap nor an answer.
    ErrorLeak,
    /// We answered exactly `any` and upstream did not. The rule ADR-0039 exists
    /// to prevent; already 7,288 lines (`checker-notes-rank.md` §6).
    AnyAnswer,
    /// Upstream printed a **name**; we printed a **structure**. We have the
    /// type and no route to its name — a printer item, not a checker item.
    TheirsNamedOursStructural,
    /// The reverse: we printed a name, upstream expanded it.
    TheirsStructuralOursNamed,
    /// The two answers are different shapes. A genuinely wrong type.
    ShapeDiffers,
    /// Same shape, different text. Usually a wrong type argument or a wrong
    /// widening — a checker item, and a narrow one.
    SameShape,
}

impl Naming {
    const fn label(self) -> &'static str {
        match self {
            Self::ErrorLeak => "error leaked into a printed type",
            Self::AnyAnswer => "we answered `any`, upstream did not",
            Self::TheirsNamedOursStructural => "upstream NAMED it, we printed a structure",
            Self::TheirsStructuralOursNamed => "upstream expanded it, we printed a name",
            Self::ShapeDiffers => "different shape — a wrong type",
            Self::SameShape => "same shape, different text",
        }
    }

    /// R3's numerator: the lines where the failure is naming rather than typing.
    const fn is_naming(self) -> bool {
        matches!(self, Self::TheirsNamedOursStructural | Self::TheirsStructuralOursNamed)
    }
}

/// A printed type that is a reference to something by name: `C`, `M.I`,
/// `Array<string>`, `typeof x`. Positive test — it must start with a name
/// character and carry none of the structural punctuation.
fn is_nominal(printed: &str) -> bool {
    printed.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && !printed.contains('{')
        && !printed.contains('(')
        && !printed.contains("=>")
        && !printed.contains('[')
        && !printed.contains('|')
        && !printed.contains('&')
}

/// A printed type that spells its own structure out.
fn is_structural(printed: &str) -> bool {
    printed.contains('{') || printed.contains("=>") || printed.contains('[')
}

fn naming(want: &str, got: &str) -> Naming {
    if got.contains("error") {
        Naming::ErrorLeak
    } else if got == "any" {
        Naming::AnyAnswer
    } else if is_nominal(want) && is_structural(got) {
        Naming::TheirsNamedOursStructural
    } else if is_structural(want) && is_nominal(got) {
        Naming::TheirsStructuralOursNamed
    } else if type_shape::classify(want) != type_shape::classify(got) {
        Naming::ShapeDiffers
    } else {
        Naming::SameShape
    }
}

/// Node kinds that are leaves by the grammar. Control C1's subject: nothing can
/// be properly contained in one, so none of them can ever be
/// `propagated/descendant`.
fn is_leaf_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Identifier
            | SyntaxKind::PrivateIdentifier
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::RegularExpressionLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword
    )
}

/// A node kind that depends on nothing at all, so a wrong answer on it is this
/// row's own fault by the grammar. `Root`'s strongest positive test.
fn is_self_contained(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::RegularExpressionLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NullKeyword
    )
}

/// Declaration kinds whose **name** is the first child the walker renders. Used
/// to find the rendered line to blame for `Prop::Declaration`.
///
/// Restricted to forms where a name exists *and* precedes the type and the
/// initialiser in source order, which is what makes "first rendered child" the
/// name rather than a guess. `ExportAssignment` and the like are excluded
/// because they have no name at all; a wrong line under one of them therefore
/// reaches `Unknown`, which is the honest answer and not `Root`.
fn name_leads(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::VariableDeclaration
            | SyntaxKind::Parameter
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::EnumDeclaration
            | SyntaxKind::EnumMember
            | SyntaxKind::ModuleDeclaration
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::PropertySignature
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::MethodSignature
            | SyntaxKind::BindingElement
            | SyntaxKind::ImportSpecifier
            | SyntaxKind::ImportClause
            | SyntaxKind::NamespaceImport
            | SyntaxKind::ExportSpecifier
            | SyntaxKind::ImportEqualsDeclaration
            | SyntaxKind::PropertyAssignment
            | SyntaxKind::ShorthandPropertyAssignment
    )
}

/// One case's contribution.
struct CaseReport {
    name: String,
    /// Upstream lines this case has.
    expected: usize,
    /// Lines we reproduced exactly — text *and* type. The gate's own test.
    matched: usize,
    /// Lines whose *text* we reproduced, so a type comparison exists.
    aligned: usize,
    /// Lines we did not align: the walker did not reproduce upstream's text, or
    /// produced nothing at that position. Control C3's subject.
    unaligned: usize,
    right: usize,
    gap: usize,
    wrong: usize,
    /// The four rankings. Every key carries the [`Prop`] label, so the ROOT-only
    /// ranking and the whole-row ranking are both re-derived at report time
    /// rather than one being inferred from the other.
    w1: HashMap<(Prop, String), usize>,
    w2: HashMap<(Prop, String), usize>,
    w3: HashMap<(Prop, String), usize>,
    w4: HashMap<(Prop, String), usize>,
    /// The naming split, per ranking-3 row, so R3 can be evaluated per row.
    naming_by_w3: HashMap<(Prop, String, Naming), usize>,
    naming_by_w4: HashMap<(Prop, String, Naming), usize>,
    /// Exact pairs for the top rows, bounded per case.
    pairs_by_w3: HashMap<(Prop, String, String, String), usize>,
    naming_total: HashMap<Naming, usize>,
    props: HashMap<Prop, usize>,
    c1_leaf_descendant: usize,
    c1_nonleaf_descendant: usize,
    /// How often the self-exclusion **fired**. A population, not a violation.
    c2_self_seen: usize,
    /// How often a declaration other than self was inspected. The mirror.
    c2_other_declaration: usize,
    /// A line classified `propagated/declaration` blaming its **own**
    /// declaration. Zero by construction — the loop `continue`s on self — and
    /// non-zero the moment that `continue` is removed, which is what makes it a
    /// control rather than decoration.
    c2_violation: usize,
    c5_sibling: usize,
}

impl CaseReport {
    fn new(name: String) -> Self {
        Self {
            name,
            expected: 0,
            matched: 0,
            aligned: 0,
            unaligned: 0,
            right: 0,
            gap: 0,
            wrong: 0,
            w1: HashMap::new(),
            w2: HashMap::new(),
            w3: HashMap::new(),
            w4: HashMap::new(),
            naming_by_w3: HashMap::new(),
            naming_by_w4: HashMap::new(),
            pairs_by_w3: HashMap::new(),
            naming_total: HashMap::new(),
            props: HashMap::new(),
            c1_leaf_descendant: 0,
            c1_nonleaf_descendant: 0,
            c2_self_seen: 0,
            c2_other_declaration: 0,
            c2_violation: 0,
            c5_sibling: 0,
        }
    }
}

/// The properties the partition rests on, checked **before** anything is
/// measured and on **every** run.
///
/// Assertions in `main` rather than `#[cfg(test)]` for the reason
/// `rank_board.rs` records: Cargo does not run tests inside an example unless
/// the manifest declares `test = true`, and `crates/tsr-conformance/Cargo.toml`
/// is shared with two other agents this cycle. A check that runs on every
/// measurement is in any case the stronger statement.
fn check_classifier() {
    // ORDER. `error[]` is *also* a shape difference and *also* nominal-vs-
    // structural. It must classify as the leak, because a leaked gap is not a
    // wrong type and must not be counted as one.
    assert_eq!(naming("string[]", "error[]"), Naming::ErrorLeak, "the leak arm must be first");
    // ORDER. `any` against a named type is also a shape difference. It must
    // classify as the rule-break, because `checker-notes-rank.md` §6 forbids
    // closing any row by widening an `any` answer and that is unenforceable if
    // those lines are hidden inside a generic bucket.
    assert_eq!(naming("C", "any"), Naming::AnyAnswer, "the `any` arm must precede the shape arm");
    // The naming pair, both directions.
    assert_eq!(naming("C", "{ a: string; }"), Naming::TheirsNamedOursStructural);
    assert_eq!(naming("{ a: string; }", "C"), Naming::TheirsStructuralOursNamed);
    assert!(Naming::TheirsNamedOursStructural.is_naming());
    assert!(!Naming::ShapeDiffers.is_naming(), "a wrong type is not a naming failure");
    assert!(!Naming::SameShape.is_naming());
    // A genuine wrong type, and a narrow one.
    assert_eq!(naming("number", "string"), Naming::SameShape);
    assert_eq!(naming("number", "C"), Naming::ShapeDiffers);
    // The nominal test must not admit a structure that happens to start with a
    // letter, or every object type becomes a naming failure.
    assert!(is_nominal("Array<string>"));
    assert!(!is_nominal("union | other"));
    assert!(is_structural("{ a: string; }"));
    assert!(!is_structural("C"));
    // C1's subject. An identifier is a leaf; an element access is not.
    assert!(is_leaf_kind(SyntaxKind::Identifier));
    assert!(!is_leaf_kind(SyntaxKind::ElementAccessExpression));
    // ROOT's strongest positive test: a literal depends on nothing.
    assert!(is_self_contained(SyntaxKind::NumericLiteral));
    assert!(!is_self_contained(SyntaxKind::Identifier), "an identifier depends on a declaration");
    // The declaration map's subject: `var x = f()` has its name first, an
    // `export = x` has no name and must not be in the map.
    assert!(name_leads(SyntaxKind::VariableDeclaration));
    assert!(!name_leads(SyntaxKind::ExportAssignment));
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

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<CaseReport> {
    // The suite's own skips, skip for skip, so every share here is a share of
    // the gradient's denominator and not of some other set.
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

    let mut report = CaseReport::new(case.name.clone());

    for (index, expected_file) in expected.iter().enumerate() {
        report.expected += expected_file.assertions.len();
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        let len = our_file.len();

        // The gate's own test, per position: text *and* type, or nothing.
        let failed: Vec<bool> = (0..len)
            .map(|i| expected_file.assertions.get(i).is_none_or(|w| w.text != our_file[i].line()))
            .collect();

        // Proper containment, not `rank_board`'s inclusive test, so control C1
        // can fire: a zero-width or identically-spanned later node is not
        // "inside" anything.
        let contains = |outer: tsr_core::Span, inner: tsr_core::Span| {
            inner.start >= outer.start
                && inner.end <= outer.end
                && (inner.start > outer.start || inner.end < outer.end)
        };
        let failing_descendant: Vec<bool> = (0..len)
            .map(|i| {
                let outer = nodes.span(line_ids[i]);
                (i + 1..len)
                    .take_while(|&j| {
                        let inner = nodes.span(line_ids[j]);
                        inner.start >= outer.start && inner.end <= outer.end
                    })
                    .any(|j| failed[j] && contains(outer, nodes.span(line_ids[j])))
            })
            .collect();

        // Rendered position of every node, and of every name-leading
        // declaration's first rendered child — which is its name, because the
        // walker is preorder and these forms put the name before the type and
        // the initialiser.
        let mut position_of: HashMap<NodeId, usize> = HashMap::with_capacity(len);
        let mut declaration_name: HashMap<NodeId, usize> = HashMap::new();
        for (position, &id) in line_ids.iter().enumerate() {
            position_of.entry(id).or_insert(position);
            if let Some(parent) = nodes.parent(id)
                && name_leads(nodes.kind(parent))
            {
                declaration_name.entry(parent).or_insert(position);
            }
        }

        for position in 0..len {
            let Some(want) = expected_file.assertions.get(position) else { continue };
            let got = &our_file[position];
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                report.unaligned += 1;
                continue;
            };
            report.aligned += 1;
            if want_type == got.type_string {
                report.matched += 1;
                report.right += 1;
                continue;
            }
            if got.type_string == "error" {
                report.gap += 1;
                continue;
            }
            report.wrong += 1;

            let id = line_ids[position];
            let kind = nodes.kind(id);
            // The sibling test, for a declaration name only: did anything
            // rendered inside the declaration this name belongs to fail? `x` in
            // `var x = f()` spans only `x`, so the span test above is blind to
            // `f()`, and calling such a line ROOT is the exact defect
            // `docs/conventions.md` records costing a row its whole ranking.
            let failing_sibling = nodes
                .parent(id)
                .filter(|&p| name_leads(nodes.kind(p)))
                .and_then(|p| declaration_name.get(&p).map(|&start| (p, start)))
                .is_some_and(|(parent, start)| {
                    let outer = nodes.span(parent);
                    (start..len)
                        .take_while(|&j| {
                            let inner = nodes.span(line_ids[j]);
                            inner.start >= outer.start && inner.end <= outer.end
                        })
                        .any(|j| j != position && failed[j])
                });
            let prop = classify(
                bound,
                nodes,
                node_map,
                id,
                kind,
                failing_descendant[position],
                failing_sibling,
                &failed,
                &declaration_name,
                &mut report,
            );
            *report.props.entry(prop).or_default() += 1;

            let name = naming(want_type, &got.type_string);
            *report.naming_total.entry(name).or_default() += 1;

            // W1 — `rank_board` §6's own key, so the two instruments can be
            // compared row for row. This is the cross-check, not a new axis.
            let w1 = format!(
                "{:?}  {} -> {}",
                kind,
                type_shape::classify(want_type).label(),
                type_shape::classify(&got.type_string).label()
            );
            // W2 — the exact substitution.
            let w2 = format!("{want_type} -> {}", got.type_string);
            // W3 — position: what the node is, by its parent. Purely
            // syntactic, no semantic naming, so there is no classifier to be
            // wrong about.
            let w3 = format!(
                "{} > {:?}",
                nodes
                    .parent(id)
                    .map_or_else(|| "(no parent)".to_string(), |p| format!("{:?}", nodes.kind(p))),
                kind
            );
            // W4 — for identifiers only, what the name resolves to. This is the
            // axis that names a work item.
            let w4 = symbol_row(bound, nodes, node_map, id, kind);

            *report.w1.entry((prop, w1)).or_default() += 1;
            *report.w2.entry((prop, w2)).or_default() += 1;
            *report.w3.entry((prop, w3.clone())).or_default() += 1;
            *report.naming_by_w3.entry((prop, w3.clone(), name)).or_default() += 1;
            if report.pairs_by_w3.len() < 4000 {
                *report
                    .pairs_by_w3
                    .entry((prop, w3, want_type.to_string(), got.type_string.clone()))
                    .or_default() += 1;
            }
            if let Some(w4) = w4 {
                *report.w4.entry((prop, w4.clone())).or_default() += 1;
                *report.naming_by_w4.entry((prop, w4, name)).or_default() += 1;
            }
        }
    }
    Some(report)
}

/// Why this wrong line is wrong here. `Root` is positive; `Unknown` is default.
#[allow(clippy::too_many_arguments)]
fn classify(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    node_map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
    kind: SyntaxKind,
    failing_descendant: bool,
    failing_sibling: bool,
    failed: &[bool],
    declaration_name: &HashMap<NodeId, usize>,
    report: &mut CaseReport,
) -> Prop {
    if failing_sibling {
        // C5's mirror: only a declaration name can reach here, because the
        // caller's test is guarded on `name_leads(parent)`.
        report.c5_sibling += 1;
        return Prop::Sibling;
    }
    if failing_descendant {
        // C1. A leaf cannot properly contain anything, so this must be 0 for
        // every leaf kind. The mirror is counted beside it.
        if is_leaf_kind(kind) {
            report.c1_leaf_descendant += 1;
        } else {
            report.c1_nonleaf_descendant += 1;
        }
        return Prop::Descendant;
    }
    // ROOT, strongest form: by the grammar this node has no inputs at all.
    if is_self_contained(kind) {
        return Prop::Root;
    }
    if kind != SyntaxKind::Identifier {
        // A non-identifier with no failing line inside it: its inputs were all
        // rendered right. Positive.
        return Prop::Root;
    }
    let Some(tsr_ast::Node::Identifier(identifier)) = node_map.get(id) else {
        return Prop::Unknown;
    };
    // `SymbolFlags::VALUE`, mirroring `gap_reason`'s meaning. A wider meaning
    // would resolve names the checker did not and blame a declaration it never
    // saw.
    let Some(symbol) = bound.resolve_name(nodes, node_map, id, identifier.text, SymbolFlags::VALUE)
    else {
        return Prop::Unknown;
    };
    let declarations = &bound.symbols().get(symbol).declarations;
    let mut saw_declaration_line = false;
    for &declaration in declarations {
        let Some(&position) = declaration_name.get(&declaration) else { continue };
        // C2. Self is excluded explicitly: a declaration name is not its own
        // dependency, and without this test every wrong declaration name would
        // blame itself and read as propagated.
        if nodes.parent(id) == Some(declaration) {
            report.c2_self_seen += 1;
            continue;
        }
        report.c2_other_declaration += 1;
        saw_declaration_line = true;
        if failed.get(position).copied().unwrap_or(false) {
            // C2's zero arm, re-tested at the point of the verdict rather than
            // inferred from the `continue` above, so removing that `continue`
            // makes this fire.
            if nodes.parent(id) == Some(declaration) {
                report.c2_violation += 1;
            }
            return Prop::Declaration;
        }
    }
    if saw_declaration_line {
        // Every declaration we could find a rendered line for was right, and
        // nothing gapped inside. Positive ROOT.
        Prop::Root
    } else {
        // The name resolved but the walker rendered no line for its
        // declaration — no evidence either way.
        Prop::Unknown
    }
}

/// The W4 row for an identifier: what the name resolves to. `None` for a node
/// that is not an identifier, so W4's denominator is the identifier lines and
/// not the whole bucket.
fn symbol_row(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    node_map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
    kind: SyntaxKind,
) -> Option<String> {
    if kind != SyntaxKind::Identifier {
        return None;
    }
    let tsr_ast::Node::Identifier(identifier) = node_map.get(id)? else { return None };
    let Some(symbol) = bound.resolve_name(nodes, node_map, id, identifier.text, SymbolFlags::VALUE)
    else {
        return Some("(does not resolve in value position)".to_string());
    };
    let data = bound.symbols().get(symbol);
    let kinds: BTreeSet<String> =
        data.declarations.iter().map(|&d| format!("{:?}", nodes.kind(d))).collect();
    let kinds = if kinds.is_empty() {
        "no declaration".to_string()
    } else {
        kinds.into_iter().collect::<Vec<_>>().join("+")
    };
    Some(format!("{:?} / {kinds}", data.flags))
}

/// Everything a row needs before it can be ranked.
#[derive(Default)]
struct Row {
    lines: usize,
    by_case: HashMap<String, usize>,
    /// For each case the row touches: what would remain if the row closed.
    residual_after: Vec<usize>,
    /// Control C3: cases scored as finished that carry an unaligned line.
    finished_with_unaligned: usize,
}

impl Row {
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.by_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let top1 = counts.first().copied().unwrap_or_default();
        let top10: usize = counts.iter().take(10).sum();
        (counts.len(), pct(top1, self.lines), pct(top10, self.lines))
    }

    fn finishes(&self) -> (usize, usize) {
        let mut rest = self.residual_after.clone();
        rest.sort_unstable();
        let finished = rest.iter().take_while(|&&r| r == 0).count();
        let median = rest.get(rest.len() / 2).copied().unwrap_or_default();
        (finished, median)
    }

    fn top_cases(&self, n: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> =
            self.by_case.iter().map(|(case, n)| (case.clone(), *n)).collect();
        entries.sort_by_key(|(case, n)| (std::cmp::Reverse(*n), case.clone()));
        entries.truncate(n);
        entries
    }
}

fn delta(left: usize, right: usize) -> i128 {
    i128::try_from(left).unwrap_or_default() - i128::try_from(right).unwrap_or_default()
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

/// Roll a per-case ranking up into rows, carrying each case's residual.
///
/// `merge_props` collapses the [`Prop`] axis, so the whole-row ranking and the
/// ROOT-only ranking are each re-derived from the per-case counts. A row's
/// `finishes` is **not** the sum of its sub-rows' — a case needing two of them
/// is finished by neither alone — which is why neither is inferred from the
/// other (`docs/conventions.md`, "before summing, state which question the sum
/// answers").
fn roll_up(
    reports: &[CaseReport],
    pick: impl Fn(&CaseReport) -> &HashMap<(Prop, String), usize>,
    only: Option<Prop>,
    merge_props: bool,
    unaligned_cases: &HashSet<&str>,
) -> HashMap<String, Row> {
    let mut rows: HashMap<String, Row> = HashMap::new();
    for case in reports {
        let residual = case.expected - case.matched;
        let mut per_key: HashMap<String, usize> = HashMap::new();
        for ((prop, key), count) in pick(case) {
            if only.is_some_and(|want| want != *prop) {
                continue;
            }
            let key = if merge_props { key.clone() } else { format!("{:<22} {key}", prop.label()) };
            *per_key.entry(key).or_default() += count;
        }
        for (key, count) in per_key {
            let row = rows.entry(key).or_default();
            row.lines += count;
            *row.by_case.entry(case.name.clone()).or_default() += count;
            let after = residual - count.min(residual);
            row.residual_after.push(after);
            if after == 0 && unaligned_cases.contains(case.name.as_str()) {
                row.finished_with_unaligned += 1;
            }
        }
    }
    rows
}

fn print_ranking(
    title: &str,
    unit: &str,
    rows: &HashMap<String, Row>,
    take: usize,
    by_lines: bool,
) {
    println!("\n### {title}");
    println!("unit: {unit}\n");
    let mut ordered: Vec<(&String, &Row)> = rows.iter().collect();
    if by_lines {
        ordered.sort_by_key(|(key, row)| (std::cmp::Reverse(row.lines), (*key).clone()));
    } else {
        ordered.sort_by_key(|(key, row)| (std::cmp::Reverse(row.finishes().0), (*key).clone()));
    }
    println!(
        "{:>7} {:>9} {:>7} {:>7} {:>7} {:>7}  row",
        "lines", "finishes", "cases", "top-1", "top-10", "median"
    );
    for (key, row) in ordered.iter().take(take) {
        let (cases, top1, top10) = row.concentration();
        let (finished, median) = row.finishes();
        println!(
            "{:>7} {:>9} {:>7} {:>6.1}% {:>6.1}% {:>7}  {}",
            row.lines, finished, cases, top1, top10, median, key
        );
        let named: Vec<String> =
            row.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
        println!("{:>48}  top: {}", "", named.join(" | "));
    }
}

#[allow(clippy::too_many_lines)]
fn report(reports: &[CaseReport]) {
    let cases = reports.len();
    let expected: usize = reports.iter().map(|r| r.expected).sum();
    let aligned: usize = reports.iter().map(|r| r.aligned).sum();
    let unaligned: usize = reports.iter().map(|r| r.unaligned).sum();
    let matched: usize = reports.iter().map(|r| r.matched).sum();
    let right: usize = reports.iter().map(|r| r.right).sum();
    let gap: usize = reports.iter().map(|r| r.gap).sum();
    let wrong: usize = reports.iter().map(|r| r.wrong).sum();
    let residual: usize = reports.iter().map(|r| r.expected - r.matched).sum();

    println!("# wrongflip — the wrong bucket, ranked by case flips\n");
    println!("cases judged:             {cases}");
    println!("assertion lines upstream: {expected}");
    println!("  aligned:                {aligned} ({:.2}%)", pct(aligned, expected));
    println!("  unaligned:              {unaligned} ({:.2}%)", pct(unaligned, expected));
    println!("  exactly right:          {matched} ({:.2}%)", pct(matched, expected));
    println!("  gap   (we said `error`):{gap:>8} ({:.2}% of residual)", pct(gap, residual));
    println!("  WRONG (ported, defect): {wrong:>8} ({:.2}% of residual)", pct(wrong, residual));
    println!("  residual:               {residual}");

    // The unaligned cases: control C3's subject.
    let unaligned_cases: HashSet<&str> =
        reports.iter().filter(|r| r.unaligned > 0).map(|r| r.name.as_str()).collect();

    println!("\n## The propagation split (unit: wrong assertion lines)\n");
    let mut props: BTreeMap<Prop, usize> = BTreeMap::new();
    for case in reports {
        for (prop, n) in &case.props {
            *props.entry(*prop).or_default() += n;
        }
    }
    for (prop, n) in &props {
        println!("  {:<24} {:>7} ({:.2}%)", prop.label(), n, pct(*n, wrong));
    }
    let root = props.get(&Prop::Root).copied().unwrap_or_default();
    println!(
        "\n  ROOT is {:.2}% of the wrong bucket. Everything else is a symptom or unmeasured.",
        pct(root, wrong)
    );

    println!("\n## Wrong type, or a name we cannot spell? (unit: wrong assertion lines)\n");
    let mut names: BTreeMap<Naming, usize> = BTreeMap::new();
    for case in reports {
        for (name, n) in &case.naming_total {
            *names.entry(*name).or_default() += n;
        }
    }
    let mut naming_total = 0usize;
    let mut naming_failures = 0usize;
    for (name, n) in &names {
        naming_total += n;
        if name.is_naming() {
            naming_failures += n;
        }
        println!("  {:<42} {:>7} ({:.2}%)", name.label(), n, pct(*n, wrong));
    }
    println!(
        "\n  naming failures (R3 numerator): {naming_failures} ({:.2}% of the bucket)",
        pct(naming_failures, wrong)
    );

    println!("\n## RANKINGS\n");
    let w1 = roll_up(reports, |r| &r.w1, None, false, &unaligned_cases);
    print_ranking(
        "W1 — node kind x shape substitution, split by cause (the `rank_board` §6 key)",
        "assertion lines; `finishes` and `cases` are cases",
        &w1,
        20,
        true,
    );
    let w1_all = roll_up(reports, |r| &r.w1, None, true, &unaligned_cases);
    print_ranking(
        "W1' — the same key, causes merged (directly comparable to `rank_board` §6)",
        "assertion lines; `finishes` and `cases` are cases",
        &w1_all,
        12,
        true,
    );
    let w2 = roll_up(reports, |r| &r.w2, Some(Prop::Root), true, &unaligned_cases);
    print_ranking(
        "W2 — the exact substitution, ROOT lines only, ranked by case flips",
        "assertion lines; `finishes` and `cases` are cases",
        &w2,
        25,
        false,
    );
    let w3 = roll_up(reports, |r| &r.w3, Some(Prop::Root), true, &unaligned_cases);
    print_ranking(
        "W3 — parent kind > node kind, ROOT lines only, ranked by case flips",
        "assertion lines; `finishes` and `cases` are cases",
        &w3,
        25,
        false,
    );
    let w4 = roll_up(reports, |r| &r.w4, Some(Prop::Root), true, &unaligned_cases);
    print_ranking(
        "W4 — what the identifier resolves to, ROOT lines only, ranked by case flips",
        "assertion lines (identifiers only); `finishes` and `cases` are cases",
        &w4,
        25,
        false,
    );
    let w4_lines = roll_up(reports, |r| &r.w4, Some(Prop::Root), true, &unaligned_cases);
    print_ranking(
        "W4' — the same rows, ranked by lines instead, so the two axes can be compared",
        "assertion lines (identifiers only); `finishes` and `cases` are cases",
        &w4_lines,
        15,
        true,
    );

    // R3 per top row, and the exact pairs — the spellability read.
    println!("\n## R3: the naming split for the top ROOT rows of W3 and W4\n");
    for (label, per_row) in [
        ("W3", collect_naming(reports, |r| &r.naming_by_w3)),
        ("W4", collect_naming(reports, |r| &r.naming_by_w4)),
    ] {
        let source = if label == "W3" { &w3 } else { &w4 };
        let mut ordered: Vec<(&String, &Row)> = source.iter().collect();
        ordered.sort_by_key(|(key, row)| (std::cmp::Reverse(row.finishes().0), (*key).clone()));
        for (key, row) in ordered.iter().take(6) {
            let Some(split) = per_row.get(*key) else { continue };
            let total: usize = split.values().sum();
            let failures: usize = split.iter().filter(|(n, _)| n.is_naming()).map(|(_, c)| c).sum();
            println!(
                "  {label} {:<58} {:>6} lines, naming {:>5} ({:.1}%)  R3 {}",
                key,
                row.lines,
                failures,
                pct(failures, total),
                if pct(failures, total) < 25.0 { "PASS" } else { "FAIL" }
            );
            let mut entries: Vec<(&Naming, &usize)> = split.iter().collect();
            entries.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
            for (name, n) in entries.iter().take(4) {
                println!("        {:>6}  {}", n, name.label());
            }
        }
    }

    println!("\n## The exact substitutions behind the top ROOT rows of W3\n");
    let mut pairs: HashMap<(String, String, String), usize> = HashMap::new();
    for case in reports {
        for ((prop, row, want, got), n) in &case.pairs_by_w3 {
            if *prop != Prop::Root {
                continue;
            }
            *pairs.entry((row.clone(), want.clone(), got.clone())).or_default() += n;
        }
    }
    let mut ordered: Vec<(&String, &Row)> = w3.iter().collect();
    ordered.sort_by_key(|(key, row)| (std::cmp::Reverse(row.finishes().0), (*key).clone()));
    for (key, _) in ordered.iter().take(4) {
        println!("  {key}");
        let mut mine: Vec<((&String, &String), usize)> = pairs
            .iter()
            .filter(|((row, _, _), _)| row == *key)
            .map(|((_, want, got), n)| ((want, got), *n))
            .collect();
        mine.sort_by_key(|(pair, n)| (std::cmp::Reverse(*n), pair.0.clone()));
        for ((want, got), n) in mine.iter().take(8) {
            println!("      {n:>6}  upstream `{want}`  ours `{got}`");
        }
    }

    println!("\n## CONTROLS\n");
    let c1_leaf: usize = reports.iter().map(|r| r.c1_leaf_descendant).sum();
    let c1_nonleaf: usize = reports.iter().map(|r| r.c1_nonleaf_descendant).sum();
    let c2_self: usize = reports.iter().map(|r| r.c2_self_seen).sum();
    let c2_violation: usize = reports.iter().map(|r| r.c2_violation).sum();
    let c2_other: usize = reports.iter().map(|r| r.c2_other_declaration).sum();
    let c5: usize = reports.iter().map(|r| r.c5_sibling).sum();
    let zero_residual = reports.iter().filter(|r| r.expected == r.matched).count();
    println!(
        "  C1  leaf kind classified propagated/descendant   = {c1_leaf} (must be 0: nothing can be properly inside a leaf)"
    );
    println!(
        "  C1' non-leaf so classified                       = {c1_nonleaf} (the mirror; pins the test rather than one arm)"
    );
    println!(
        "  C2  a line classified propagated/declaration blaming its OWN declaration = {c2_violation} (must be 0)"
    );
    println!(
        "  C2' the self-exclusion fired                     = {c2_self} (population, NOT a violation — proves the exclusion is live)"
    );
    println!("  C2\" another declaration inspected                = {c2_other} (the mirror)");
    println!(
        "  C5  declaration names blamed on a sibling        = {c5} (the sibling arm's population)"
    );
    // Pinned by another instrument rather than by arithmetic inside this one:
    // `coverage` reports 2,173 passing cases at `058b4a9`. `checker-notes-rank.md`
    // §7 records this residual metric reading +18 against the gate, because it
    // cannot see lines *we* emit that upstream does not. So the expected value
    // is 2,173 plus a small positive number, and a *negative* difference or a
    // large one is a defect in this probe.
    println!(
        "  C6  cases with residual 0                        = {zero_residual} (the gate reports 2,173 at 058b4a9; +small is expected, negative or large is a defect)"
    );
    let c3: usize = [&w1, &w1_all, &w2, &w3, &w4]
        .iter()
        .flat_map(|rows| rows.values())
        .map(|row| row.finished_with_unaligned)
        .sum();
    println!(
        "  C3  a row scored `finishes` in a case that has an unaligned line = {c3} (must be 0 by construction)"
    );
    println!(
        "\n  A1  right+gap+wrong-aligned                      = {} (must be 0)",
        delta(right + gap + wrong, aligned)
    );
    let w1_sum: usize = w1_all.values().map(|row| row.lines).sum();
    let w2_sum: usize =
        reports.iter().flat_map(|r| r.w2.iter()).map(|((_, _), n)| n).sum::<usize>();
    let w3_sum: usize =
        reports.iter().flat_map(|r| r.w3.iter()).map(|((_, _), n)| n).sum::<usize>();
    println!("  A2  W1 row sum - wrong total                     = {}", delta(w1_sum, wrong));
    println!("  A2' W2 row sum - wrong total                     = {}", delta(w2_sum, wrong));
    println!("  A2\" W3 row sum - wrong total                     = {}", delta(w3_sum, wrong));
    println!(
        "  A3  propagation split sum - wrong total          = {}",
        delta(props.values().sum::<usize>(), wrong)
    );
    println!("  A4  naming split sum - wrong total               = {}", delta(naming_total, wrong));
    println!(
        "\n  NOT A CONTROL: the naming classifier is total, so no bucket of it can read zero as"
    );
    println!("  evidence of a partition. Stated rather than left for a reader to discover.");
}

fn collect_naming(
    reports: &[CaseReport],
    pick: impl Fn(&CaseReport) -> &HashMap<(Prop, String, Naming), usize>,
) -> HashMap<String, HashMap<Naming, usize>> {
    let mut out: HashMap<String, HashMap<Naming, usize>> = HashMap::new();
    for case in reports {
        for ((prop, row, name), n) in pick(case) {
            if *prop != Prop::Root {
                continue;
            }
            *out.entry(row.clone()).or_default().entry(*name).or_default() += n;
        }
    }
    out
}
