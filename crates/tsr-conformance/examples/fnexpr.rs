//! What stops a function expression from having a type, and what fixing it
//! would actually convert.
//!
//! # The population, named before the rule is registered
//!
//! `docs/conventions.md` records a rule whose top-1 leg read 44.5% on the direct
//! row and 65.3% combined, with the rule never saying which. So, first:
//!
//! > **P — the population every rule on this page is registered over — is every
//! > `.types` assertion line this port renders whose **node kind** is
//! > `ArrowFunction` or `FunctionExpression`.** Whole-gradient, all outcomes
//! > (right, gap and wrong), not restricted to any walk and not restricted to
//! > the lines that currently gap.
//!
//! `ObjectLiteralExpression` and `ArrayLiteralExpression` are measured beside it
//! because the same table answers them for free, and they are **not in P**: they
//! are `objects.rs` and `array_literals.rs`, which belong to another workstream.
//! No rule here is registered over them and no number here should be quoted as
//! though one were.
//!
//! **P is syntactic, and that is the point.** A checker change cannot alter which
//! nodes exist, so `|P|` is fixed across any two runs of this probe — control C0.
//! That is what makes a before/after pair comparable line for line, which a
//! population defined as *"the lines that gap"* would not be: the gap set moves
//! under the very change being measured.
//!
//! # The rules, pre-registered before the first run
//!
//! `docs/conventions.md`: *size the conversion, not the population*. An earlier
//! item on this project was sized at 1,784 lines and converted 362, because a
//! name resolving is necessary and not sufficient for the assertion line to
//! match. So neither rule below is written on a count of lines that *become
//! computable*; both are written on lines that **match the baseline**.
//!
//! - **RC (size, direct).** Build only if the counterfactual run converts
//!   **≥25% of P's currently-gapping lines** — that is, `right` over P rises by
//!   at least that much. The counterfactual is described below and it is a
//!   measurement, not an estimate.
//! - **R2 (spellability / quality, direct).** Of P's lines that **stop gapping**
//!   in the counterfactual, **≥70% must match the baseline exactly.** Spellability
//!   has correctly refused three builds on this project; an inferred return type
//!   is precisely the kind of answer that is easy to compute and hard to name.
//!
//! Conjunctive. RC sizes the win, R2 gates its quality, and both read off buckets
//! this probe prints rather than off anything derived from them.
//!
//! # The counterfactual
//!
//! `get_type_of_function_expression` (`crates/tsr-checker/src/signatures.rs:1010`)
//! answers `errorType` when any parameter is unannotated and the node is not
//! demonstrably free of a contextual type (`:1014`). That guard is the candidate
//! blocker. `docs/architecture/checker-notes-calls.md` is explicit that probing a
//! blocker is too weak — *"pick the shape, hardcode past the blocker, and see
//! whether the form then answers"* — so the sizing run is this probe re-run
//! against a checker with that guard relaxed, by a **named mutation applied and
//! reverted**. The relaxation is not committed; what is committed is the number.
//!
//! # Controls, printed unconditionally
//!
//! - **C0 — `|P|` is fixed by the grammar.** The count of rendered lines per node
//!   kind cannot be changed by a checker edit. Two runs whose `|P|` differs are
//!   not a before/after pair, and the number is printed so the pair can be
//!   checked rather than assumed.
//! - **C1 — a gapping line in P carries a matching `gap_reason`.** `gap_reason`
//!   ends `expression answered error: {kind}` for exactly these forms, so the
//!   count of P's gap lines whose reason names a *different* kind is **0 by
//!   construction**; it reads non-zero if `type_at_location` and `gap_reason`
//!   drift apart.
//! - **C2 — `this` parameters, as a pair.** Registered as *"an arrow function
//!   cannot have a `this` parameter, so this reads 0 by the grammar"*, with the
//!   same count over `FunctionExpression` beside it as the mirror.
//!   **It fired, at 4, and the premise was wrong.** *An arrow function cannot
//!   have a 'this' parameter* is a **checker diagnostic**, TS2730, reported at
//!   `vendor/typescript-go/internal/checker/checker.go:2688` from
//!   `diagnostics.An_arrow_function_cannot_have_a_this_parameter`
//!   (`diagnostics_generated.go:1729`) — the parser builds the node and the
//!   checker rejects it. So the count is real: 4 arrow functions in the corpus
//!   that upstream errors on. Kept, restated as a diagnostic rather than a
//!   grammar control, and the pair (4 against 14) still does its original job of
//!   distinguishing "the arm is right" from "the arm never fires".
//! - **C3 — the gate classifier has no loaded default.** Its last arm,
//!   `Gate::SignatureBody`, is a *positive* test (every earlier gate is false and
//!   the node is function-like), and the two failure arms are positive tests on
//!   the node map. The loaded label — `Gate::ContextualGuard`, the one the
//!   counterfactual targets — is first and syntactic.
//! - **A1 — the outcome split partitions P.** Arithmetic.
//!
//! Run: `cargo run --release -p tsr-conformance --example fnexpr`

use std::collections::BTreeMap;

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The four node kinds this page tallies. Only the first two are in P.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Form {
    Arrow,
    FunctionExpression,
    ObjectLiteral,
    ArrayLiteral,
}

impl Form {
    const ALL: [Self; 4] =
        [Self::Arrow, Self::FunctionExpression, Self::ObjectLiteral, Self::ArrayLiteral];

    const fn of(kind: SyntaxKind) -> Option<Self> {
        match kind {
            SyntaxKind::ArrowFunction => Some(Self::Arrow),
            SyntaxKind::FunctionExpression => Some(Self::FunctionExpression),
            SyntaxKind::ObjectLiteralExpression => Some(Self::ObjectLiteral),
            SyntaxKind::ArrayLiteralExpression => Some(Self::ArrayLiteral),
            _ => None,
        }
    }

    /// Whether the form is in P — the population the rules are registered over.
    const fn in_p(self) -> bool {
        matches!(self, Self::Arrow | Self::FunctionExpression)
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Arrow => "ArrowFunction            (P, signatures.rs)",
            Self::FunctionExpression => "FunctionExpression       (P, signatures.rs)",
            Self::ObjectLiteral => "ObjectLiteralExpression  (not P, objects.rs)",
            Self::ArrayLiteral => "ArrayLiteralExpression   (not P, array_literals.rs)",
        }
    }
}

/// What this port answered for a line, against upstream's baseline.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Outcome {
    /// Our rendered line equals upstream's, character for character.
    Right,
    /// We answered `error` and upstream did not.
    Gap,
    /// We answered something and it is not what upstream says.
    Wrong,
}

impl Outcome {
    const fn label(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Gap => "gap",
            Self::Wrong => "wrong",
        }
    }
}

/// Which gate in `get_type_of_function_expression` /
/// `get_signature_from_declaration` a gapping line most likely stopped at,
/// decided **syntactically**, in the checker's own order.
///
/// The order is load-bearing and is the checker's: the contextual guard
/// (`signatures.rs:1014`) runs before anything reads a parameter's name, so a
/// function with both an unannotated parameter and a binding pattern is stopped
/// by the guard and belongs in the first bucket.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Gate {
    /// A parameter has no annotation and the node is not demonstrably free of a
    /// contextual type. `get_type_of_function_expression` returns `errorType`
    /// here (`signatures.rs:1014`). **This is the gate the counterfactual lifts.**
    ContextualGuard,
    /// A parameter's name is a binding pattern, so `parameter_of` cannot render
    /// it (`signatures.rs:766`).
    BindingPattern,
    /// The declaration is generic, so `type_parameter_of` has to succeed for
    /// every parameter.
    Generic,
    /// Every syntactic gate above is clear, so the failure is inside the
    /// signature build — the return type or a parameter annotation that gapped.
    /// A positive test, not a residue: it requires all three tests above to be
    /// false **and** the node to be function-like.
    SignatureBody,
    /// The node map does not hold the node. A positive failure of the walk.
    NoNode,
    /// The node is in P by kind but is not function-like when read back. A
    /// positive failure.
    NotFunctionLike,
}

impl Gate {
    const fn label(self) -> &'static str {
        match self {
            Self::ContextualGuard => {
                "unannotated parameter, contextual type not excluded  <- the counterfactual"
            }
            Self::BindingPattern => "a parameter's name is a binding pattern",
            Self::Generic => "the function is generic",
            Self::SignatureBody => "syntactic gates clear; the signature build failed",
            Self::NoNode => "NO NODE (walk failed)",
            Self::NotFunctionLike => "NOT FUNCTION-LIKE (walk failed)",
        }
    }
}

/// A bucket's lines and the cases they came from, for concentration.
#[derive(Default)]
struct Tally {
    lines: usize,
    per_case: BTreeMap<String, usize>,
}

impl Tally {
    fn add(&mut self, case: &str, n: usize) {
        self.lines += n;
        *self.per_case.entry(case.to_string()).or_default() += n;
    }

    fn merge(&mut self, other: &Self) {
        self.lines += other.lines;
        for (case, n) in &other.per_case {
            *self.per_case.entry(case.clone()).or_default() += n;
        }
    }

    fn top_cases(&self, n: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> =
            self.per_case.iter().map(|(case, n)| (case.clone(), *n)).collect();
        entries.sort_by_key(|(case, n)| (std::cmp::Reverse(*n), case.clone()));
        entries.truncate(n);
        entries
    }

    /// `(cases, top-1, top-10)` as shares of `lines`.
    #[allow(clippy::cast_precision_loss)]
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.per_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let total = self.lines.max(1) as f64;
        let top1 = counts.first().copied().unwrap_or(0) as f64 / total * 100.0;
        let top10 = counts.iter().take(10).sum::<usize>() as f64 / total * 100.0;
        (counts.len(), top1, top10)
    }
}

#[derive(Default)]
struct Report {
    right: usize,
    gap: usize,
    wrong: usize,
    /// C0 and A1: `|P|` by form and outcome.
    forms: BTreeMap<(Form, Outcome), Tally>,
    /// Why the gapping lines gap, syntactically.
    gates: BTreeMap<(Form, Gate), Tally>,
    /// The verbatim `gap_reason` of P's gap lines, to confirm the row identity.
    reasons: BTreeMap<String, Tally>,
    /// The baseline's own RHS for P's gap lines, verbatim, by gate.
    rhs: BTreeMap<(Gate, String), usize>,
    /// The shape of that RHS: does it even look like a function type?
    rhs_shape: BTreeMap<(Form, &'static str), usize>,
    /// What we answered on P's **wrong** lines, against what upstream says.
    wrong_pairs: BTreeMap<(String, String), usize>,
    /// P's wrong lines that a single rule would fix: our answer becomes
    /// upstream's exactly when every `undefined` / `null` in it is replaced by
    /// `any`. That is `getReturnTypeFromBody`'s widening
    /// (`getWidenedType`, applied at `checker.go:20211`), which this port does
    /// not apply to a `null` or `undefined` return. Direct, and computed from the
    /// two strings rather than from a claim about the cause.
    widening_would_fix: usize,
    /// The mirror: P's wrong lines the same substitution does **not** fix.
    widening_would_not_fix: usize,
    /// The same question asked **whole-gradient**, over every node kind, because
    /// a return type is printed by function declarations and methods too and the
    /// rule would not be able to tell them apart.
    widen_gain: Tally,
    /// **The damage side.** Lines we already get right whose answer carries a
    /// standalone `undefined` or `null` inside a function type — every one of
    /// them is a line an unconditional widening rule would break. Upstream
    /// widens `undefined`/`null` to `any` only when `strictNullChecks` is
    /// **off**, and the corpus sets `@strict: true` on part of itself.
    widen_risk: Tally,
    /// The same pair restricted to the **return position** — the only position an
    /// inferred-return-type widening can reach. The whole-string versions above
    /// are upper bounds on both sides, because they also count a `null` in a
    /// parameter, which no return rule would touch.
    widen_gain_return: Tally,
    widen_risk_return: Tally,
    c1_reason_kind_mismatch: usize,
    c1_reason_kind_match: usize,
    c2_arrow_this_parameter: usize,
    c2_function_this_parameter: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.right += other.right;
        self.gap += other.gap;
        self.wrong += other.wrong;
        for (key, tally) in &other.forms {
            self.forms.entry(*key).or_default().merge(tally);
        }
        for (key, tally) in &other.gates {
            self.gates.entry(*key).or_default().merge(tally);
        }
        for (key, tally) in &other.reasons {
            self.reasons.entry(key.clone()).or_default().merge(tally);
        }
        for (key, n) in &other.rhs {
            *self.rhs.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.rhs_shape {
            *self.rhs_shape.entry(*key).or_default() += n;
        }
        for (key, n) in &other.wrong_pairs {
            *self.wrong_pairs.entry(key.clone()).or_default() += n;
        }
        self.widening_would_fix += other.widening_would_fix;
        self.widening_would_not_fix += other.widening_would_not_fix;
        self.widen_gain.merge(&other.widen_gain);
        self.widen_risk.merge(&other.widen_risk);
        self.widen_gain_return.merge(&other.widen_gain_return);
        self.widen_risk_return.merge(&other.widen_risk_return);
        self.c1_reason_kind_mismatch += other.c1_reason_kind_mismatch;
        self.c1_reason_kind_match += other.c1_reason_kind_match;
        self.c2_arrow_this_parameter += other.c2_arrow_this_parameter;
        self.c2_function_this_parameter += other.c2_function_this_parameter;
    }
}

/// The shape of a baseline right-hand side, for a population whose answers are
/// all function types. `Spell::Plain` from the call-resolution page is useless
/// here — every correct answer contains `=>` — so the test asks the question that
/// is actually open: **does upstream print a function type at all?**
fn rhs_shape(rhs: &str) -> &'static str {
    if rhs == "any" {
        "any"
    } else if rhs.starts_with('(') || rhs.starts_with('<') {
        "a function type"
    } else if rhs.contains("=>") {
        "contains => but does not start as one"
    } else {
        "not a function type"
    }
}

/// Whether replacing every `undefined` and `null` in our answer with `any`
/// yields upstream's, exactly.
///
/// Word-boundary aware, so `undefinedThing` and `nullable` are untouched — a
/// naive `replace` would claim lines this rule cannot fix and inflate the bucket
/// the decision is read off.
fn widening_fixes(ours: &str, upstream: &str) -> bool {
    if ours == upstream {
        return false;
    }
    let mut out = String::with_capacity(ours.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word == "undefined" || word == "null" {
            out.push_str("any");
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for ch in ours.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            word.push(ch);
        } else {
            flush(&mut word, &mut out);
            out.push(ch);
        }
    }
    flush(&mut word, &mut out);
    out == upstream
}

/// Whether a printed function type carries a standalone `undefined` or `null`,
/// which is what an unconditional widening rule would rewrite.
fn carries_a_bare_nullish(text: &str) -> bool {
    if !text.contains("=>") {
        return false;
    }
    let mut word = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            word.push(ch);
        } else {
            if word == "undefined" || word == "null" {
                return true;
            }
            word.clear();
        }
    }
    false
}

/// Our answer with a trailing `=> undefined` / `=> null` rewritten to `=> any`,
/// or `None` when the answer does not end that way.
///
/// The **return position** only. `getWidenedType` under `strictNullChecks: false`
/// maps `undefinedType` and `nullType` to `anyType`
/// (`getWidenedTypeWithContext`, `checker.go`), and an inferred return type is
/// the only place this port could apply it without also rewriting an annotation
/// the source wrote down.
fn widen_return_position(ours: &str) -> Option<String> {
    if !ours.starts_with('(') && !ours.starts_with('<') {
        return None;
    }
    for suffix in ["=> undefined", "=> null"] {
        if let Some(head) = ours.strip_suffix(suffix) {
            return Some(format!("{head}=> any"));
        }
    }
    None
}

fn check_classifiers() {
    assert_eq!(widen_return_position("() => undefined").as_deref(), Some("() => any"));
    assert_eq!(widen_return_position("<T>(x: T) => null").as_deref(), Some("<T>(x: T) => any"));
    assert_eq!(widen_return_position("() => void"), None);
    assert_eq!(widen_return_position("undefined"), None, "not a function type");
    assert_eq!(
        widen_return_position("(x: null) => void"),
        None,
        "a nullish parameter is not the return position"
    );
    assert!(carries_a_bare_nullish("() => undefined"));
    assert!(carries_a_bare_nullish("(x: string | null) => void"));
    assert!(!carries_a_bare_nullish("() => nullable"), "word-boundary aware");
    assert!(!carries_a_bare_nullish("undefined"), "not a function type; the rule cannot reach it");
    assert!(widening_fixes("() => undefined", "() => any"));
    assert!(widening_fixes("<T>(x: T) => null", "<T>(x: T) => any"));
    assert!(!widening_fixes("() => undefined", "() => string"));
    assert!(!widening_fixes("() => any", "() => any"), "an identical pair is not a wrong line");
    assert!(
        !widening_fixes("(x: undefinedThing) => null", "(x: any) => any"),
        "the substitution is word-boundary aware"
    );
    assert_eq!(Form::of(SyntaxKind::ArrowFunction), Some(Form::Arrow));
    assert_eq!(Form::of(SyntaxKind::FunctionExpression), Some(Form::FunctionExpression));
    assert_eq!(Form::of(SyntaxKind::CallExpression), None);
    assert!(Form::Arrow.in_p());
    assert!(!Form::ObjectLiteral.in_p(), "objects.rs is another workstream's file");

    assert_eq!(rhs_shape("(x: number) => string"), "a function type");
    assert_eq!(rhs_shape("<T>(x: T) => T"), "a function type");
    assert_eq!(rhs_shape("any"), "any");
    assert_eq!(rhs_shape("void"), "not a function type");
    assert_eq!(rhs_shape("{ f: () => void; }"), "contains => but does not start as one");
}

fn main() {
    check_classifiers();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let reports: Vec<Report> = cases.par_iter().filter_map(measure).collect();
    let mut total = Report::default();
    for report in reports {
        total.merge(&report);
    }
    print(&total);
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));

    let mut report = Report::default();
    let name = &case.name;

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }

        for (position, assertion) in our_file.iter().enumerate() {
            // The baseline is asked first. A line where we answer `error` and
            // upstream's baseline also says `error` is a **right** answer;
            // `receiver_gap.rs` had this backwards and misfiled 389 lines
            // (`9b10272`, `bd tsr-zlo`).
            let baseline = expected_file.assertions.get(position);
            let outcome = if baseline.is_some_and(|b| b.text == assertion.line()) {
                report.right += 1;
                Outcome::Right
            } else if assertion.type_string == "error" {
                report.gap += 1;
                Outcome::Gap
            } else {
                report.wrong += 1;
                Outcome::Wrong
            };

            // The widening question, whole-gradient and independent of P: a
            // return-type rule cannot tell a function expression's line from a
            // function declaration's, so both halves are counted over every kind.
            if let Some(line) = baseline
                && let Some(upstream) = line.text.strip_prefix(&format!("{} : ", assertion.text))
            {
                if outcome == Outcome::Wrong && widening_fixes(&assertion.type_string, upstream) {
                    report.widen_gain.add(name, 1);
                }
                if outcome == Outcome::Right && carries_a_bare_nullish(&assertion.type_string) {
                    report.widen_risk.add(name, 1);
                }
                if let Some(widened) = widen_return_position(&assertion.type_string) {
                    if outcome == Outcome::Wrong && widened == upstream {
                        report.widen_gain_return.add(name, 1);
                    }
                    if outcome == Outcome::Right {
                        report.widen_risk_return.add(name, 1);
                    }
                }
            }

            let id = line_ids[position];
            let Some(form) = Form::of(nodes.kind(id)) else { continue };
            report.forms.entry((form, outcome)).or_default().add(name, 1);

            // C2: an arrow function cannot declare a `this` parameter; a
            // function expression can. Printed as a pair.
            if form.in_p()
                && let Some(first) = parameters_of(map, id).and_then(|p| p.first().copied())
                && let Some(tsr_ast::BindingName::Identifier(binding)) = first.name
                && binding.text == "this"
            {
                if form == Form::Arrow {
                    report.c2_arrow_this_parameter += 1;
                } else {
                    report.c2_function_this_parameter += 1;
                }
            }

            // The baseline's own right-hand side, where the walkers agree on the
            // expression text. This is the only spellability evidence that does
            // not depend on anything we answer.
            let rhs = baseline.and_then(|line| {
                line.text.strip_prefix(&format!("{} : ", assertion.text)).map(ToString::to_string)
            });

            if outcome == Outcome::Wrong && form.in_p() {
                let upstream = rhs.clone().unwrap_or_else(|| "<walkers disagree>".to_string());
                if widening_fixes(&assertion.type_string, &upstream) {
                    report.widening_would_fix += 1;
                } else {
                    report.widening_would_not_fix += 1;
                }
                *report
                    .wrong_pairs
                    .entry((assertion.type_string.clone(), upstream))
                    .or_default() += 1;
            }

            if outcome != Outcome::Gap || !form.in_p() {
                continue;
            }

            let reason = types_producer::gap_reason(&mut checker, bound, nodes, map, id);
            // C1: `gap_reason`'s expression arm ends with the node's own kind.
            if reason.ends_with(&format!("{:?}", nodes.kind(id))) {
                report.c1_reason_kind_match += 1;
            } else {
                report.c1_reason_kind_mismatch += 1;
            }
            report.reasons.entry(reason).or_default().add(name, 1);

            let gate = classify(nodes, map, id);
            report.gates.entry((form, gate)).or_default().add(name, 1);
            if let Some(rhs) = rhs {
                *report.rhs_shape.entry((form, rhs_shape(&rhs))).or_default() += 1;
                *report.rhs.entry((gate, rhs)).or_default() += 1;
            }
        }
    }
    Some(report)
}

/// The parameter list of a function-like node in P.
fn parameters_of<'a>(
    map: &tsr_ast::NodeMap<'a>,
    id: NodeId,
) -> Option<&'a [&'a tsr_ast::ParameterDeclaration<'a>]> {
    match map.get(id) {
        Some(Node::ArrowFunction(node)) => Some(node.parameters),
        Some(Node::FunctionExpression(node)) => Some(node.parameters),
        _ => None,
    }
}

/// The type-parameter list of a function-like node in P.
fn type_parameters_of<'a>(
    map: &tsr_ast::NodeMap<'a>,
    id: NodeId,
) -> Option<&'a [&'a tsr_ast::TypeParameterDeclaration<'a>]> {
    match map.get(id) {
        Some(Node::ArrowFunction(node)) => Some(node.type_parameters),
        Some(Node::FunctionExpression(node)) => Some(node.type_parameters),
        _ => None,
    }
}

/// Which gate stops this function expression, in the checker's own order.
fn classify(nodes: &tsr_ast::NodeTable, map: &tsr_ast::NodeMap<'_>, id: NodeId) -> Gate {
    if map.get(id).is_none() {
        return Gate::NoNode;
    }
    let (Some(parameters), Some(type_parameters)) =
        (parameters_of(map, id), type_parameters_of(map, id))
    else {
        return Gate::NotFunctionLike;
    };
    // `signatures.rs:1013` — `parts.parameters.iter().any(|p| p.r#type.is_none())`
    // — and `:1014`'s `!self.has_no_contextual_type(node)`, reproduced verbatim
    // below rather than approximated, because a looser test here would attribute
    // lines to a gate the checker did not take.
    if parameters.iter().any(|parameter| parameter.r#type.is_none())
        && !has_no_contextual_type(nodes, map, id)
    {
        return Gate::ContextualGuard;
    }
    // `parameter_of` (`signatures.rs:766`) returns `None` for any name that is
    // not a plain identifier.
    if parameters
        .iter()
        .any(|parameter| !matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(_))))
    {
        return Gate::BindingPattern;
    }
    if !type_parameters.is_empty() {
        return Gate::Generic;
    }
    Gate::SignatureBody
}

/// `Checker::has_no_contextual_type` (`crates/tsr-checker/src/signatures.rs:753`),
/// reproduced. The initialiser of a `var`/`let`/`const` with no annotation is the
/// one position the checker will call contextual-type-free.
fn has_no_contextual_type(
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    declaration: NodeId,
) -> bool {
    let Some(parent) = nodes.parent(declaration) else { return false };
    matches!(
        map.get(parent),
        Some(Node::VariableDeclaration(node))
            if node.r#type.is_none()
                && node.initializer.and_then(|i| Node::from(i).node_id()) == Some(declaration)
    )
}

#[allow(clippy::too_many_lines, clippy::cast_precision_loss, clippy::cast_possible_wrap)]
fn print(report: &Report) {
    let total = report.right + report.gap + report.wrong;
    println!("# fnexpr — what stops a function expression, and what fixing it converts\n");
    println!("The unit is an ASSERTION LINE everywhere on this page.\n");
    println!(
        "gradient (lines we rendered; NOT the suite's denominator, see reconcile.rs):\n  \
         {} = right {} ({:.2}%) + gap {} ({:.2}%) + wrong {} ({:.2}%)\n",
        total,
        report.right,
        report.right as f64 / total.max(1) as f64 * 100.0,
        report.gap,
        report.gap as f64 / total.max(1) as f64 * 100.0,
        report.wrong,
        report.wrong as f64 / total.max(1) as f64 * 100.0,
    );

    println!("## 1. C0 / A1 — |P| by form and outcome\n");
    println!("{:<44} {:>7} {:>7} {:>7} {:>8}", "form", "right", "gap", "wrong", "TOTAL");
    let mut p_gap = 0usize;
    let mut p_right = 0usize;
    let mut p_wrong = 0usize;
    for form in Form::ALL {
        let get = |outcome: Outcome| report.forms.get(&(form, outcome)).map_or(0, |t| t.lines);
        let (right, gap, wrong) = (get(Outcome::Right), get(Outcome::Gap), get(Outcome::Wrong));
        println!(
            "{:<44} {:>7} {:>7} {:>7} {:>8}",
            form.label(),
            right,
            gap,
            wrong,
            right + gap + wrong
        );
        if form.in_p() {
            p_right += right;
            p_gap += gap;
            p_wrong += wrong;
        }
    }
    println!(
        "\n|P| = {} lines  (right {}, gap {}, wrong {})\n",
        p_right + p_gap + p_wrong,
        p_right,
        p_gap,
        p_wrong
    );
    println!("  RC's denominator is P's gap: {p_gap}");
    println!("  RC fires when a counterfactual run's right-over-P rises by >= {}\n", p_gap / 4);

    println!("## 2. Concentration, per form and outcome\n");
    for form in Form::ALL {
        for outcome in [Outcome::Right, Outcome::Gap, Outcome::Wrong] {
            let Some(tally) = report.forms.get(&(form, outcome)) else { continue };
            if tally.lines == 0 {
                continue;
            }
            let (cases, top1, top10) = tally.concentration();
            println!(
                "  {:<44} {:<6} {:>6} lines {:>5}c  top-1 {:>5.1}%  top-10 {:>5.1}%",
                form.label(),
                outcome.label(),
                tally.lines,
                cases,
                top1,
                top10
            );
            let named: Vec<String> =
                tally.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
            println!("        top: {}", named.join(" | "));
        }
    }
    println!();

    println!("## 3. The verbatim gap_reason of P's gap lines\n");
    let mut reasons: Vec<_> = report.reasons.iter().collect();
    reasons.sort_by_key(|(_, tally)| std::cmp::Reverse(tally.lines));
    for (reason, tally) in reasons.iter().take(10) {
        let (cases, top1, top10) = tally.concentration();
        println!(
            "{:>6} {:>5}c {:>6.1}%t1 {:>6.1}%t10  {}",
            tally.lines, cases, top1, top10, reason
        );
    }
    println!();

    println!("## 4. Which gate stops them, in the checker's own order\n");
    let mut by_gate: BTreeMap<Gate, Tally> = BTreeMap::new();
    for ((form, gate), tally) in &report.gates {
        if form.in_p() {
            by_gate.entry(*gate).or_default().merge(tally);
        }
    }
    let gate_total: usize = by_gate.values().map(|t| t.lines).sum();
    println!("{:<62} {:>7} {:>7} {:>6} {:>8}", "gate", "lines", "share", "cases", "top-10");
    for (gate, tally) in &by_gate {
        let (cases, _, top10) = tally.concentration();
        println!(
            "{:<62} {:>7} {:>6.1}% {:>6} {:>7.1}%",
            gate.label(),
            tally.lines,
            tally.lines as f64 / gate_total.max(1) as f64 * 100.0,
            cases,
            top10
        );
        let named: Vec<String> =
            tally.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
        println!("      top: {}", named.join(" | "));
    }
    println!("{:<62} {:>7}\n", "GATE TOTAL", gate_total);
    println!("  per form:");
    for form in Form::ALL {
        if !form.in_p() {
            continue;
        }
        for ((f, gate), tally) in &report.gates {
            if *f == form {
                println!("      {:<44} {:>6}  {}", form.label(), tally.lines, gate.label());
            }
        }
    }
    println!();

    println!("## 5. R2's evidence — what the BASELINE prints for P's gap lines\n");
    for form in Form::ALL {
        if !form.in_p() {
            continue;
        }
        let total: usize =
            report.rhs_shape.iter().filter(|((f, _), _)| *f == form).map(|(_, n)| *n).sum();
        if total == 0 {
            continue;
        }
        println!("  {} — {} lines with a comparable baseline RHS", form.label(), total);
        for ((f, shape), n) in &report.rhs_shape {
            if *f == form {
                println!("      {:>6}  {:>5.1}%  {}", n, *n as f64 / total as f64 * 100.0, shape);
            }
        }
    }
    println!();
    println!("  the verbatim RHS behind the counterfactual's gate:");
    let mut rhs: Vec<_> =
        report.rhs.iter().filter(|((gate, _), _)| *gate == Gate::ContextualGuard).collect();
    rhs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((_, text), n) in rhs.iter().take(12) {
        println!("      {n:>5}  {text}");
    }
    println!();
    println!("  and behind the signature-build gate:");
    let mut rhs: Vec<_> =
        report.rhs.iter().filter(|((gate, _), _)| *gate == Gate::SignatureBody).collect();
    rhs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((_, text), n) in rhs.iter().take(12) {
        println!("      {n:>5}  {text}");
    }
    println!();

    println!("## 6. P's lines we already answer WRONG — ours vs upstream\n");
    let mut pairs: Vec<_> = report.wrong_pairs.iter().collect();
    pairs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((ours, upstream), n) in pairs.iter().take(12) {
        println!("      {n:>5}  ours {ours}\n             them {upstream}");
    }
    println!();

    println!(
        "  a single rule — widen a `null`/`undefined` return to `any` — fixes {} of {} P wrong lines ({:.1}%)",
        report.widening_would_fix,
        report.widening_would_fix + report.widening_would_not_fix,
        report.widening_would_fix as f64
            / (report.widening_would_fix + report.widening_would_not_fix).max(1) as f64
            * 100.0
    );
    println!();

    println!("## 6b. The widening rule, whole-gradient: both sides\n");
    let (gain_cases, gain_top1, gain_top10) = report.widen_gain.concentration();
    let (risk_cases, risk_top1, risk_top10) = report.widen_risk.concentration();
    println!(
        "  GAIN  wrong lines a `null`/`undefined` -> `any` return widening fixes exactly: {} over {} cases (top-1 {:.1}%, top-10 {:.1}%)",
        report.widen_gain.lines, gain_cases, gain_top1, gain_top10
    );
    for (case, n) in report.widen_gain.top_cases(5) {
        println!("          {n:>5}  {case}");
    }
    println!(
        "  RISK  right lines carrying a bare `undefined`/`null` in a function type: {} over {} cases (top-1 {:.1}%, top-10 {:.1}%)",
        report.widen_risk.lines, risk_cases, risk_top1, risk_top10
    );
    for (case, n) in report.widen_risk.top_cases(5) {
        println!("          {n:>5}  {case}");
    }
    println!(
        "  ratio risk/gain = {:.2}",
        report.widen_risk.lines as f64 / report.widen_gain.lines.max(1) as f64
    );
    let (gr_cases, gr_top1, gr_top10) = report.widen_gain_return.concentration();
    let (rr_cases, rr_top1, _) = report.widen_risk_return.concentration();
    println!(
        "\n  RETURN POSITION ONLY — the rule a checker could actually write:\n             GAIN {} over {} cases (top-1 {:.1}%, top-10 {:.1}%)\n             RISK {} over {} cases (top-1 {:.1}%)\n    ratio risk/gain = {:.2}",
        report.widen_gain_return.lines,
        gr_cases,
        gr_top1,
        gr_top10,
        report.widen_risk_return.lines,
        rr_cases,
        rr_top1,
        report.widen_risk_return.lines as f64 / report.widen_gain_return.lines.max(1) as f64
    );
    for (case, n) in report.widen_gain_return.top_cases(5) {
        println!("          gain {n:>5}  {case}");
    }
    for (case, n) in report.widen_risk_return.top_cases(5) {
        println!("          risk {n:>5}  {case}");
    }
    println!();

    println!("## 7. Controls\n");
    println!(
        "  C0  |P| (fixed by the grammar; must be identical across a pair) = {}",
        p_right + p_gap + p_wrong
    );
    println!(
        "  C1  a P gap line whose gap_reason names another kind = {} (must be 0)",
        report.c1_reason_kind_mismatch
    );
    println!(
        "  C1' the mirror                                       = {} (pins the arm)",
        report.c1_reason_kind_match
    );
    println!(
        "  C2  an ArrowFunction with a `this` parameter          = {} (upstream errors: TS2730, checker.go:2688)",
        report.c2_arrow_this_parameter
    );
    println!(
        "  C2' a FunctionExpression with one (the mirror)        = {} (legal; pins the arm)",
        report.c2_function_this_parameter
    );
    println!(
        "  A1  P's outcomes {} == gate total {} over the gap half ({} vs {})",
        p_right + p_gap + p_wrong,
        gate_total,
        p_gap,
        gate_total
    );
}
