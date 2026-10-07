//! Leg 1 of `bd tsr-kmzf`'s registered bar — the counterfactual for a
//! three-valued assignability relation
//! (`docs/architecture/checker-notes-assign.md` §3).
//!
//! `examples/selectable.rs` measured **492** gap lines sitting on a call to a
//! 2+-declaration function, of which **303** stop because a candidate's
//! parameter is an object type and **44** because it is a union. The bar
//! registers those two plus `namedcallee.rs`'s 48 as **one** population, and
//! asks one question before any `calls.rs` code is written:
//!
//! > for every one of them, (a) does the ternary relation *decide* the pair,
//! > and (b) if it decides it, does the decision agree with the baseline?
//!
//! **Keep if the forecast converts ≥ 150. The registration names the falsifier
//! as the likely outcome**, on the grounds that overload sets which differ by
//! object parameter type characteristically differ by optional members and by
//! signature-bearing shapes — rows 2 and 6 of §2's table, the two `Unknown`
//! swallows most.
//!
//! # This forecasts the real predicate, not a model of it
//!
//! `selectable.rs` reconstructs `SELECTABLE` from the public flags because it
//! is a private constant, and asserts the reconstruction against the real one.
//! That is not available here: a decidability predicate is not a flag set, it is
//! the walk itself, and a probe-side model of it would forecast *the model*.
//! So [`Checker::relate_ternary`] is the real walk, and this probe calls it.
//!
//! It was landed behaviour-neutral first, which is the control that makes the
//! forecast readable: `is_type_related_to` is defined as
//! `relate_ternary(..) == Related`, and `checker_types` reads **2,657 / 71.14%**
//! across the change — identical to `STATUS.md` §1. A refactor that moved the
//! gradient would mean the two walks had diverged and every number below would
//! be measuring the divergence.
//!
//! # What the forecast models
//!
//! Upstream's `chooseOverload` (`internal/checker/checker.go`) walks the
//! candidates **in declaration order** and takes the first whose parameters all
//! accept their arguments. So the ternary decides a *call* — not merely a pair —
//! exactly when it can carry out that scan without ever needing an answer it
//! does not have:
//!
//! - a candidate whose every pair is `Related` **selects**, provided every
//!   earlier candidate was `NotRelated`;
//! - any `Unknown` reached at or before the selected candidate makes the whole
//!   call **undecided**, because the scan cannot know whether that candidate
//!   would have won;
//! - all candidates `NotRelated` is decided too, but it is not a conversion —
//!   upstream reports an error there, and this port has no diagnostics.
//!
//! Note the asymmetry that makes this cheaper than it looks: an `Unknown` on a
//! candidate *after* the winner costs nothing, because the scan has already
//! stopped. A population figure counted per-pair would miss that and understate
//! the item.
//!
//! # Refusals modelled, each because a gap beats a wrong answer
//!
//! - **a rest or optional parameter** — arity is decidable syntactically, but
//!   the mapping from arguments to parameters is not modelled here, so a
//!   candidate carrying one is refused rather than approximated;
//! - **a generic candidate** — that is inference, `callgate.rs`'s own largest
//!   gate at 2,324 lines and a different item;
//! - **an argument whose type gaps** — nothing to relate;
//! - **a parameter annotation that gaps** — likewise.
//!
//! # Controls
//!
//! - **C1** every classified line answers `errorType` today. Expected **0**
//!   violations, inherited from `selectable.rs` where it read 0.
//! - **C2** the buckets sum to the classified count.
//! - **C3** — pinned by the *upstream construct*, not by arithmetic. Under the
//!   **binary** relation every one of these calls fails to select, or the line
//!   would not be a gap. So re-running the same scan with `Unknown` folded back
//!   to `NotRelated` — which is exactly what `is_type_assignable_to` does today —
//!   must select **zero** calls that convert. A non-zero reading means the probe
//!   is reaching lines the shipped gate already reaches, i.e. the population is
//!   not the one the bar was registered against. An arithmetic control cannot
//!   see this; this one is fixed by what the compiler already does.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_checker::relater::{Relation, Ternary};
use tsr_checker::{Checker, TypeFlags};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// What the scan concluded about one call.
enum Outcome {
    /// A candidate was selected, and this is the text its return annotation
    /// would print.
    Selected(String),
    /// Every candidate was rejected. Decided, but not an answer this port can
    /// print — upstream reports an error here.
    NoneApplicable,
    /// The scan needed an answer the relation does not have, with the reason.
    Undecided(&'static str),
}

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    /// The bucket that decides the item: a gap this would turn into a wrong
    /// line, keyed by `want -> got` so the families are visible.
    wrong: BTreeMap<String, usize>,
    converted_cases: BTreeMap<String, usize>,
    /// The outcome cross-tabulated against `selectable.rs`'s shape bucket —
    /// the split C3 forced. `(shape, outcome) -> lines`.
    shaped: BTreeMap<(String, String), usize>,
    c1_not_gap: usize,
    /// C3 — conversions the *binary* relation would already have made.
    c3_binary_converts: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        self.c3_binary_converts += other.c3_binary_converts;
        for (k, n) in &other.forms {
            *self.forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.wrong {
            *self.wrong.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.converted_cases {
            *self.converted_cases.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.shaped {
            *self.shaped.entry(k.clone()).or_default() += n;
        }
    }
}

/// The parameter annotations of a function-like declaration, or a refusal.
fn candidate_parameters<'a>(
    checker: &mut Checker<'a, '_>,
    map: &tsr_ast::NodeMap<'a>,
    declaration: NodeId,
) -> Result<Vec<tsr_checker::TypeId>, &'static str> {
    let (parameters, type_parameters) = match map.get(declaration) {
        Some(Node::FunctionDeclaration(f)) => (f.parameters, f.type_parameters),
        Some(Node::MethodDeclaration(m)) => (m.parameters, m.type_parameters),
        _ => return Err("  a candidate this probe cannot read"),
    };
    if !type_parameters.is_empty() {
        return Err("  a GENERIC candidate — inference, a different item");
    }
    let error = checker.intrinsics().error;
    let mut resolved = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        // A POSITIONAL refusal, priced rather than assumed — the `bd tsr-4sa`
        // move, which took a 646-convert/377-wrong design to 625/2 by refusing
        // families instead of tuning a ratio.
        //
        // `any` is assignable to everything and everything to `any`
        // (`isSimpleTypeRelatedTo`, `internal/checker/relater.go`), so a
        // candidate carrying an `any` parameter is *trivially* applicable and
        // the declaration-order scan always stops on it. Upstream still picks
        // correctly there because it has argument-order and inference machinery
        // this port does not; selecting the first candidate is therefore a
        // WRONG RULE, not a bad trade, and a rule is not priced.
        if parameter.dot_dot_dot_token.is_some() || parameter.question_token.is_some() {
            return Err("  a rest or optional parameter — arity not modelled");
        }
        let Some(annotation) = parameter.r#type else {
            return Err("  a parameter with no annotation");
        };
        let id = checker.get_type_from_type_node(annotation);
        if id == error {
            return Err("  a parameter annotation that GAPS");
        }
        if checker.type_of(id).flags.intersects(TypeFlags::ANY) {
            return Err("  an `any` parameter — REFUSED positionally");
        }
        resolved.push(id);
    }
    Ok(resolved)
}

/// `selectable.rs`'s shape bucket for a candidate set — the first parameter
/// annotation outside `SELECTABLE`, named the way that instrument names it.
///
/// Recomputed here rather than joined on, so the cross-tab below is over one
/// pass and one reachability rule. C3 is what made this necessary: the two
/// shapes it separates are blocked by the **gate**, not by the relation, and a
/// bar registered over their union is void if they behave differently
/// (`docs/conventions.md`, "a bar registered against a population is a mixture").
fn shape_of<'a>(
    checker: &mut Checker<'a, '_>,
    map: &tsr_ast::NodeMap<'a>,
    declarations: &[NodeId],
) -> &'static str {
    let selectable = TypeFlags::STRING
        .union(TypeFlags::STRING_LITERAL)
        .union(TypeFlags::NUMBER)
        .union(TypeFlags::NUMBER_LITERAL)
        .union(TypeFlags::BIG_INT)
        .union(TypeFlags::BIG_INT_LITERAL)
        .union(TypeFlags::BOOLEAN)
        .union(TypeFlags::BOOLEAN_LITERAL)
        .union(TypeFlags::VOID)
        .union(TypeFlags::UNDEFINED)
        .union(TypeFlags::NULL)
        .union(TypeFlags::NEVER);
    let error = checker.intrinsics().error;
    for &declaration in declarations {
        let parameters = match map.get(declaration) {
            Some(Node::FunctionDeclaration(f)) => f.parameters,
            Some(Node::MethodDeclaration(m)) => m.parameters,
            _ => continue,
        };
        for parameter in parameters {
            let Some(annotation) = parameter.r#type else { return "no annotation" };
            let resolved = checker.get_type_from_type_node(annotation);
            if resolved == error {
                return "annotation gaps";
            }
            let flags = checker.type_of(resolved).flags;
            if !selectable.contains(flags) {
                return if flags.intersects(TypeFlags::UNION) {
                    "UNION param"
                } else if flags.intersects(TypeFlags::INTERSECTION) {
                    "INTERSECTION param"
                } else if flags.intersects(TypeFlags::TYPE_PARAMETER) {
                    "TYPE PARAM"
                } else if flags.intersects(TypeFlags::ENUM_LIKE) {
                    "ENUM param"
                } else if flags.intersects(TypeFlags::ES_SYMBOL_LIKE) {
                    "SYMBOL param"
                } else if flags.intersects(TypeFlags::ANY) {
                    "`any` param"
                } else if flags.intersects(TypeFlags::OBJECT) {
                    "OBJECT param"
                } else {
                    "another shape"
                };
            }
        }
    }
    "all selectable"
}

/// What the selected candidate's return would print, resolved through
/// [`Checker::get_type_from_type_node`] and rendered the way a `.types`
/// baseline renders a call.
///
/// Reading the written annotation directly keeps the counterfactual independent
/// of the plumbing it is sizing — `namedcallee.rs` took the same route for the
/// same reason. `None` where there is no annotation (return-type *inference* is
/// a separate mechanism) or where the annotation gaps.
fn written_return_of<'a>(
    checker: &mut Checker<'a, '_>,
    map: &tsr_ast::NodeMap<'a>,
    declaration: NodeId,
) -> Option<String> {
    let annotation = match map.get(declaration) {
        Some(Node::FunctionDeclaration(f)) => f.r#type,
        Some(Node::MethodDeclaration(m)) => m.r#type,
        _ => None,
    }?;
    let id = checker.get_type_from_type_node(annotation);
    (id != checker.intrinsics().error).then(|| checker.type_to_string(id))
}

/// Upstream's `chooseOverload` scan, run over a relation of the caller's
/// choosing. `ternary == false` folds `Unknown` back to `NotRelated`, which is
/// what the shipped `is_type_assignable_to` does — control C3.
fn choose_overload(
    checker: &mut Checker<'_, '_>,
    candidates: &[(NodeId, Vec<tsr_checker::TypeId>, Option<String>)],
    arguments: &[tsr_checker::TypeId],
    ternary: bool,
) -> Outcome {
    for (_, parameters, written_return) in candidates {
        if parameters.len() != arguments.len() {
            // Arity is decided syntactically; a mismatch is a real rejection.
            continue;
        }
        let mut verdict = Ternary::Related;
        for (&argument, &parameter) in arguments.iter().zip(parameters) {
            let pair = checker.relate_ternary(argument, parameter, Relation::Assignable);
            let pair = match (pair, ternary) {
                (Ternary::Unknown, false) => Ternary::NotRelated,
                (other, _) => other,
            };
            match pair {
                Ternary::NotRelated => {
                    verdict = Ternary::NotRelated;
                    break;
                }
                Ternary::Unknown => verdict = Ternary::Unknown,
                Ternary::Related => {}
            }
        }
        match verdict {
            Ternary::Related => {
                return match written_return {
                    Some(text) => Outcome::Selected(text.clone()),
                    None => Outcome::Undecided("  the winner's return annotation GAPS"),
                };
            }
            // The scan cannot pass this candidate: it does not know whether
            // this one would have won, so no later candidate can be selected.
            Ternary::Unknown => {
                return Outcome::Undecided("  UNDECIDED — a pair the relation cannot decide");
            }
            Ternary::NotRelated => {}
        }
    }
    Outcome::NoneApplicable
}

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
    let error = checker.intrinsics().error;

    let mut report = Report::default();
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            if want.text == got.line() {
                continue;
            }
            // `selectable.rs`'s reachability rule, verbatim. A different rule
            // would make the two populations incomparable and void the bar.
            let Some(wanted) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if got.type_string != "error" {
                continue;
            }
            let id = line_ids[position];
            let Some(Node::CallExpression(call)) = map.get(id) else { continue };
            let Some(tsr_ast::Expression::Identifier(callee)) = call.expression else { continue };
            let Some(callee_id) = callee.node_id else { continue };
            let Some(symbol) =
                bound.resolve_name(nodes, map, callee_id, callee.text, SymbolFlags::VALUE)
            else {
                continue;
            };
            let declarations: Vec<_> = bound
                .symbols()
                .get(symbol)
                .declarations
                .iter()
                .copied()
                .filter(|&d| {
                    matches!(
                        nodes.kind(d),
                        SyntaxKind::FunctionDeclaration | SyntaxKind::MethodDeclaration
                    )
                })
                .collect();
            if declarations.len() < 2 {
                continue;
            }
            report.classified += 1;
            let shape = shape_of(&mut checker, map, &declarations).to_string();
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }

            // Resolve the candidates, then the arguments. A refusal on any
            // candidate refuses the call: answering off a knowingly incomplete
            // candidate set is a wrong rule, not a bad trade (`bd tsr-4sa`).
            let mut candidates = Vec::with_capacity(declarations.len());
            let mut refusal = None;
            for declaration in declarations {
                match candidate_parameters(&mut checker, map, declaration) {
                    Ok(parameters) => {
                        let written = written_return_of(&mut checker, map, declaration);
                        candidates.push((declaration, parameters, written));
                    }
                    Err(reason) => {
                        refusal = Some(reason);
                        break;
                    }
                }
            }
            if let Some(reason) = refusal {
                *report.forms.entry(reason.to_string()).or_default() += 1;
                *report.shaped.entry((shape, reason.to_string())).or_default() += 1;
                continue;
            }

            let mut arguments = Vec::with_capacity(call.arguments.len());
            let mut argument_gap = false;
            for argument in call.arguments {
                let Some(argument_id) = Node::from(*argument).node_id() else {
                    argument_gap = true;
                    break;
                };
                let id = types_producer::type_id_at_location(
                    &mut checker,
                    bound,
                    nodes,
                    map,
                    argument_id,
                );
                if id == error || checker.type_of(id).flags.is_empty() {
                    argument_gap = true;
                    break;
                }
                arguments.push(id);
            }
            if argument_gap {
                *report.forms.entry("  an ARGUMENT whose type gaps".to_string()).or_default() += 1;
                *report
                    .shaped
                    .entry((shape, "  an ARGUMENT whose type gaps".to_string()))
                    .or_default() += 1;
                continue;
            }

            let outcome = choose_overload(&mut checker, &candidates, &arguments, true);
            match outcome {
                Outcome::Selected(text) => {
                    let binary = matches!(
                        choose_overload(&mut checker, &candidates, &arguments, false),
                        Outcome::Selected(ref b) if *b == text
                    );
                    let suffix =
                        if binary { " [binary decides it too]" } else { " [NEEDS the ternary]" };
                    if text == wanted {
                        *report
                            .shaped
                            .entry((shape.clone(), format!("CONVERTS{suffix}")))
                            .or_default() += 1;
                        *report.forms.entry("  CONVERTS".to_string()).or_default() += 1;
                        *report.converted_cases.entry(case.name.clone()).or_default() += 1;
                    } else {
                        *report
                            .shaped
                            .entry((shape.clone(), format!("WRONG{suffix}")))
                            .or_default() += 1;
                        *report
                            .forms
                            .entry("  WOULD PRINT WRONG — the column that decides".to_string())
                            .or_default() += 1;
                        *report.wrong.entry(format!("want {wanted}  got {text}")).or_default() += 1;
                    }
                    // C3: would the binary relation have selected this too?
                    if let Outcome::Selected(binary) =
                        choose_overload(&mut checker, &candidates, &arguments, false)
                        && binary == wanted
                    {
                        report.c3_binary_converts += 1;
                    }
                }
                Outcome::NoneApplicable => {
                    *report
                        .forms
                        .entry("  decided: NO candidate applies (upstream errors)".to_string())
                        .or_default() += 1;
                    *report.shaped.entry((shape, "none applicable".to_string())).or_default() += 1;
                }
                Outcome::Undecided(reason) => {
                    *report.forms.entry(reason.to_string()).or_default() += 1;
                    *report.shaped.entry((shape, reason.to_string())).or_default() += 1;
                }
            }
        }
    }
    Some(report)
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");
    let mut report = Report::default();
    for partial in cases.par_iter().filter_map(measure).collect::<Vec<_>>() {
        report.merge(&partial);
    }

    println!("# ternary — leg 1 of `bd tsr-kmzf` (docs/architecture/checker-notes-assign.md §3)\n");
    println!(
        "classified (gap line on a call to a 2+-declaration function): {}\n",
        report.classified
    );
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}{form}");
    }
    let converts = report.forms.get("  CONVERTS").copied().unwrap_or(0);
    let wrong =
        report.forms.get("  WOULD PRINT WRONG — the column that decides").copied().unwrap_or(0);
    println!("\n## The bar\n");
    println!("  leg 1 floor           150");
    println!(
        "  forecast CONVERTS     {converts}   -> {}",
        if converts >= 150 { "PASS" } else { "FALSIFIER FIRES" }
    );
    println!("  would print WRONG     {wrong}   (leg 4's absolute is 10, measured after a build)");
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);
    println!(
        "  C3 conversions the BINARY relation already makes: {}  (expect 0)",
        report.c3_binary_converts
    );

    let mut wrongs: Vec<_> = report.wrong.iter().collect();
    wrongs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## The at-risk column, by family\n");
    for (family, n) in wrongs.into_iter().take(15) {
        println!("  {n:>6}  {family}");
    }

    println!("\n## The cross-tab C3 forced: outcome by `selectable.rs` shape\n");
    let mut shaped: Vec<_> = report.shaped.iter().collect();
    shaped.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((shape, outcome), n) in shaped {
        println!("  {n:>6}  {shape:<20} {outcome}");
    }

    let mut cases: Vec<_> = report.converted_cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Where the conversions are\n");
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
