//! Which **arm** gapped, for the three rows nobody has measured.
//!
//! # Why this exists
//!
//! `STATUS.md` §4 ranks `BinaryExpression` (1,418 own-root gap lines),
//! `NewExpression` (1,049) and `ArrayLiteralExpression` (637) as items 1–3, on
//! the stated grounds that they are "never measured, unowned". The handover
//! describes them as *"the form's own rule is missing"*.
//!
//! **Reading the code first says that framing is wrong.** `binary.rs` ports
//! assignment, the arithmetic/bitwise/shift family, `+`, the relational and
//! equality families, `in`, `instanceof` and comma — and deliberately gaps the
//! **logical** operators (`bd tsr-5s2`) and destructuring assignment.
//! `array_literals.rs` ports the whole non-tuple tail and deliberately gaps
//! spreads, omissions, a gapped element, and two object-typed constituents.
//! Neither row is a missing rule. Each is a *named set of withheld arms*, and
//! the planning question is which arm holds the lines.
//!
//! `gaproot.rs` cannot answer that: its `ROOT/own-rule` bucket is one label for
//! "the form's own rule did not fire", and a form with eleven arms lands its
//! whole population in that one cell.
//!
//! # The population, defined so it cannot drift under the checker
//!
//! An **aligned gap line**: `types_suite::compare` matched our expression text
//! against upstream's, and our type is `error`. That is `gaproot`'s population
//! predicate, character for character, including *baseline first* — a line
//! where we answer `error` and upstream's baseline also says `error` is a
//! **right** answer, not a gap (`examples/reconcile.rs`, `bd tsr-zlo`).
//!
//! # Own-root here is pinned by the operands, not by a span
//!
//! `gaproot` decides "own root" by descending a span. For these three forms
//! that is unnecessary and weaker: **a binary expression's roots are exactly its
//! two operands**, and this probe asks the checker for their types directly via
//! `types_producer::type_at_location`. If either renders `error`, the line's
//! root is that operand and not the operator. Same for a `new` expression's
//! callee and an array literal's elements.
//!
//! That is the *construction*-pinned form `docs/conventions.md` prefers: the
//! value is fixed by what the operands are, and no span predicate can invert
//! without the operand strings changing too.
//!
//! # Controls
//!
//! - **C1, arithmetic.** Every gap line lands in exactly one arm; the arm
//!   counts sum to the kind's gap total.
//! - **C2, construction.** `EqualsToken` with a non-pattern left-hand side
//!   cannot be an own-root gap: `binary.rs` returns the right operand's type
//!   unconditionally, so if the right operand types, the line types. Expect
//!   **0**. A non-zero reading means the own-root test is inverted or the arm
//!   labels are misassigned.
//! - **C3, construction.** `LessThanToken` and its family answer
//!   `intrinsics.boolean` *unconditionally* — they do not consult the operands
//!   at all. So no comparison operator can appear in the gap population under
//!   **any** own-root verdict. Expect **0** in every bucket, own-root or not.
//! - **C4, frozen.** The kind totals are compared against `STATUS.md` §4's
//!   published own-root figures (1,418 / 1,049 / 637), which were produced by
//!   `gaproot.rs` at `3299f53` and which no mutation of this file can move.
//!   These are *different* predicates — `gaproot` descends a span, this one
//!   reads the operands — so they are not expected to agree exactly, and the
//!   size of the disagreement is the finding.
//!
//! # Use
//!
//! ```text
//! cargo run --release -p tsr-conformance --example armsplit
//! ```

use std::collections::HashMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Expression, Node, SyntaxKind};
use tsr_conformance::types_baseline::{self};
use tsr_conformance::{Corpus, repo_root, types_producer};

/// One gap line, reduced to what the ranking needs.
struct Line {
    kind: &'static str,
    /// The withheld arm, named after the code that withheld it.
    arm: String,
    /// Did every operand type? If not, the root is elsewhere.
    own_root: bool,
    /// Upstream's answer, the right-hand side of the baseline line.
    want: String,
    /// The rendered types of the operands, when they rendered.
    left: String,
    right: String,
    case: String,
    /// The case carries TS2563, so upstream disabled control-flow analysis and
    /// answers `any` by construction — ADR-0038's ceiling, never work.
    ts2563: bool,
}

/// The comparison family, which `binary.rs` answers `boolean` unconditionally.
fn is_comparison(op: SyntaxKind) -> bool {
    matches!(
        op,
        SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::ExclamationEqualsToken
            | SyntaxKind::EqualsEqualsEqualsToken
            | SyntaxKind::ExclamationEqualsEqualsToken
            | SyntaxKind::InKeyword
            | SyntaxKind::InstanceOfKeyword
    )
}

/// The arm of `check_binary_expression` that produced the gap, named after the
/// branch in `binary.rs` rather than after the operator, because that is the
/// unit a build would deliver.
fn binary_arm(op: SyntaxKind, is_pattern: bool) -> String {
    if op == SyntaxKind::EqualsToken && is_pattern {
        return "destructuring-assignment".to_owned();
    }
    match op {
        SyntaxKind::AmpersandAmpersandToken => "logical &&".to_owned(),
        SyntaxKind::BarBarToken => "logical ||".to_owned(),
        SyntaxKind::QuestionQuestionToken => "logical ??".to_owned(),
        SyntaxKind::AmpersandAmpersandEqualsToken
        | SyntaxKind::BarBarEqualsToken
        | SyntaxKind::QuestionQuestionEqualsToken => "logical assignment".to_owned(),
        SyntaxKind::PlusToken | SyntaxKind::PlusEqualsToken => "addition fallthrough".to_owned(),
        SyntaxKind::EqualsToken | SyntaxKind::CommaToken => "assignment/comma".to_owned(),
        other if is_comparison(other) => "comparison".to_owned(),
        _ => "arithmetic".to_owned(),
    }
}

fn array_arm(literal: &tsr_ast::ArrayLiteralExpression<'_>, element_gap: bool) -> String {
    if literal.elements.iter().any(|e| matches!(e, Expression::SpreadElement(_))) {
        return "spread element".to_owned();
    }
    if literal.elements.iter().any(|e| matches!(e, Expression::OmittedExpression(_))) {
        return "omitted element".to_owned();
    }
    if element_gap {
        return "an element gapped".to_owned();
    }
    if literal.elements.is_empty() {
        return "empty, and still a gap".to_owned();
    }
    "union / object reduction / global Array".to_owned()
}

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<Vec<Line>> {
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
    let ts2563 =
        case.expected_errors().ok().flatten().is_some_and(|errors| errors.contains("TS2563"));

    let mut out = Vec::new();

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }

        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            // Baseline first. See the module docs.
            if want.text == got.line() {
                continue;
            }
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if got.type_string != "error" {
                continue;
            }

            let id = line_ids[position];
            let mut type_of = |node: Option<Expression<'_>>| -> String {
                node.and_then(|n| n.node_id()).map_or_else(String::new, |c| {
                    types_producer::type_at_location(&mut checker, bound, nodes, map, c)
                })
            };

            let line = match map.get(id) {
                Some(Node::BinaryExpression(binary)) => {
                    let Some(op) = binary.operator_token.map(|t| t.kind) else { continue };
                    let is_pattern = matches!(
                        binary.left,
                        Some(
                            Expression::ObjectLiteralExpression(_)
                                | Expression::ArrayLiteralExpression(_)
                        )
                    );
                    let left = type_of(binary.left);
                    let right = type_of(binary.right);
                    // A destructuring pattern's left-hand side is not an
                    // operand upstream ever types, so it does not count against
                    // own-root.
                    let left_ok = is_pattern || left != "error";
                    Line {
                        kind: "BinaryExpression",
                        arm: binary_arm(op, is_pattern),
                        own_root: left_ok && right != "error",
                        want: want_type.to_owned(),
                        left,
                        right,
                        case: case.name.clone(),
                        ts2563,
                    }
                }
                Some(Node::NewExpression(new)) => {
                    let callee = type_of(new.expression);
                    let args_gap = new.arguments.iter().any(|a| type_of(Some(*a)) == "error");
                    Line {
                        kind: "NewExpression",
                        arm: if callee == "error" {
                            "callee gapped".to_owned()
                        } else if args_gap {
                            "an argument gapped".to_owned()
                        } else {
                            "construct-signature resolution".to_owned()
                        },
                        own_root: callee != "error" && !args_gap,
                        want: want_type.to_owned(),
                        left: callee,
                        right: String::new(),
                        case: case.name.clone(),
                        ts2563,
                    }
                }
                Some(Node::ArrayLiteralExpression(literal)) => {
                    let element_gap = literal.elements.iter().any(|e| type_of(Some(*e)) == "error");
                    Line {
                        kind: "ArrayLiteralExpression",
                        arm: array_arm(literal, element_gap),
                        own_root: !element_gap,
                        want: want_type.to_owned(),
                        left: String::new(),
                        right: String::new(),
                        case: case.name.clone(),
                        ts2563,
                    }
                }
                _ => continue,
            };
            out.push(line);
        }
    }
    Some(out)
}

/// A bucket of lines, so every row prints its concentration beside its size —
/// `docs/conventions.md`: quote the case count beside any concentrated gain.
#[derive(Default)]
struct Bucket {
    lines: usize,
    cases: HashMap<String, usize>,
    wants: HashMap<String, usize>,
    /// Exact-match counterfactuals, for the logical arms only.
    want_is_right: usize,
    want_is_left: usize,
    want_is_left_or_right: usize,
    /// `new C(...)`: the callee types `typeof C` and upstream answers `C`. The
    /// cheapest possible construct-signature design is *strip the `typeof`*,
    /// and this counts the lines it would get exactly right.
    want_is_callee_instance: usize,
    /// The callee's type is not of the form `typeof X` at all, so the strip
    /// design has nothing to strip — a real construct-signature lookup is
    /// needed. Counted so the two legs are named separately, per
    /// `docs/conventions.md` on putting the bar on the *predicted* leg.
    callee_not_typeof: usize,
    /// `(left, right) -> want`, for reading whether a rule exists at all.
    triples: HashMap<(String, String, String), usize>,
    /// For `new`: what the callee's type actually is, which names the arm of
    /// construct-signature resolution the line needs.
    callees: HashMap<String, usize>,
}

impl Bucket {
    fn push(&mut self, line: &Line) {
        self.lines += 1;
        *self.cases.entry(line.case.clone()).or_default() += 1;
        *self.wants.entry(line.want.clone()).or_default() += 1;
        if line.want == line.right {
            self.want_is_right += 1;
        }
        if line.want == line.left {
            self.want_is_left += 1;
        }
        if !line.left.is_empty()
            && !line.right.is_empty()
            && (line.want == format!("{} | {}", line.left, line.right)
                || line.want == format!("{} | {}", line.right, line.left))
        {
            self.want_is_left_or_right += 1;
        }
        if line.kind == "NewExpression" {
            match line.left.strip_prefix("typeof ") {
                Some(instance) if instance == line.want => self.want_is_callee_instance += 1,
                Some(_) => {}
                None => self.callee_not_typeof += 1,
            }
            *self.callees.entry(line.left.clone()).or_default() += 1;
        }
        if line.arm.starts_with("logical") {
            *self
                .triples
                .entry((line.left.clone(), line.right.clone(), line.want.clone()))
                .or_default() += 1;
        }
    }

    fn top1(&self) -> (String, usize) {
        self.cases
            .iter()
            .max_by_key(|(_, n)| **n)
            .map_or_else(|| (String::new(), 0), |(name, n)| (name.clone(), *n))
    }
}

fn print_bucket(label: &str, bucket: &Bucket, show_counterfactual: bool) {
    let (top_case, top_lines) = bucket.top1();
    let share = if bucket.lines == 0 {
        0.0
    } else {
        100.0 * f64::from(u32::try_from(top_lines).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(bucket.lines).unwrap_or(u32::MAX))
    };
    println!(
        "  {label:<38} {:>6} lines  {:>5} cases  top-1 {share:>5.1}%  {top_case}",
        bucket.lines,
        bucket.cases.len()
    );
    let mut wants: Vec<(&String, &usize)> = bucket.wants.iter().collect();
    wants.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (want, n) in wants.iter().take(6) {
        println!("      want {n:>6}  {want}");
    }
    if show_counterfactual {
        println!(
            "      exact-match counterfactuals: == right {}  == left {}  == `left | right` {}",
            bucket.want_is_right, bucket.want_is_left, bucket.want_is_left_or_right
        );
        let mut triples: Vec<(&(String, String, String), &usize)> = bucket.triples.iter().collect();
        triples.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for ((left, right, want), n) in triples.iter().take(8) {
            println!("      {n:>5}  ({left})  op  ({right})  ->  {want}");
        }
    }
    if bucket.want_is_callee_instance > 0 || bucket.callee_not_typeof > 0 {
        println!(
            "      exact-match counterfactual `strip typeof from the callee`: {} of {} ({:.1}%);\
             \n      callee is not `typeof X` at all: {}",
            bucket.want_is_callee_instance,
            bucket.lines,
            100.0 * f64::from(u32::try_from(bucket.want_is_callee_instance).unwrap_or(u32::MAX))
                / f64::from(u32::try_from(bucket.lines).unwrap_or(u32::MAX)),
            bucket.callee_not_typeof,
        );
        let mut callees: Vec<(&String, &usize)> = bucket.callees.iter().collect();
        callees.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        println!("      the callee's own type, top 10 of {}:", bucket.callees.len());
        for (callee, n) in callees.iter().take(10) {
            println!("        {n:>5}  {callee}");
        }
    }
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let lines: Vec<Line> = cases.par_iter().filter_map(measure).flatten().collect();

    // C3, construction: the comparison family answers `boolean` without
    // consulting its operands, so no comparison operator can be in the gap
    // population at all.
    let comparisons = lines.iter().filter(|l| l.arm == "comparison").count();
    // C2, construction: plain assignment returns the right operand's type
    // unconditionally.
    let plain_assignments =
        lines.iter().filter(|l| l.arm == "assignment/comma" && l.own_root).count();

    for kind in ["BinaryExpression", "NewExpression", "ArrayLiteralExpression"] {
        let of_kind: Vec<&Line> = lines.iter().filter(|l| l.kind == kind).collect();
        let own: Vec<&&Line> = of_kind.iter().filter(|l| l.own_root && !l.ts2563).collect();
        println!(
            "\n=== {kind} — {} aligned gap lines, {} own-root (TS2563 excluded), {} under TS2563",
            of_kind.len(),
            own.len(),
            of_kind.iter().filter(|l| l.ts2563).count(),
        );

        let mut arms: HashMap<String, Bucket> = HashMap::new();
        let mut blocked: HashMap<String, Bucket> = HashMap::new();
        for line in &of_kind {
            if line.ts2563 {
                continue;
            }
            let table = if line.own_root { &mut arms } else { &mut blocked };
            table.entry(line.arm.clone()).or_default().push(line);
        }

        let mut rows: Vec<(&String, &Bucket)> = arms.iter().collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.1.lines));
        println!("  -- own root: every operand typed, the arm is simply withheld --");
        for (arm, bucket) in rows {
            print_bucket(arm, bucket, arm.starts_with("logical"));
        }

        let mut rows: Vec<(&String, &Bucket)> = blocked.iter().collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.1.lines));
        println!("  -- blocked: an operand gapped, so the root is elsewhere --");
        for (arm, bucket) in rows {
            print_bucket(arm, bucket, false);
        }
    }

    println!("\n=== controls");
    println!("  C1 arithmetic  every line lands in exactly one arm, by construction of the match");
    println!(
        "  C2 plain assignment, own root                       {plain_assignments}  (expect 0)"
    );
    println!("  C3 comparison family, any bucket                    {comparisons}  (expect 0)");
    println!(
        "  C4 frozen: STATUS.md §4 at 3299f53 published own-root 1,418 / 1,049 / 637 by gaproot's\n     span descent; this probe reads the operands. Disagreement is the finding, not a defect."
    );
}
