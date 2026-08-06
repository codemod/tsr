//! Counterfactual for structural type-argument inference — `bd tsr-g30h`.
//!
//! `callgate.rs` sized one gate at **2,324 lines**: a single call candidate
//! resolves and *inference* is what stops (`checker-notes-callres.md` §13.2).
//! A population is a ceiling, so this probe does not count that row. It
//! **computes the string the arm would print and compares it to the baseline,
//! character for character**.
//!
//! # The slice being forecast
//!
//! `inferTypes` (`internal/checker/inference.go:53`) is a walk of a *source*
//! type against a *target* type accumulating candidates. This port stores a
//! type's payload as printed text, so almost none of that walk is expressible
//! here — with exactly two exceptions, both of which are reverse indices that
//! already exist because substitution needed them:
//!
//! - [`Checker::type_reference_target`] — the `(symbol, arguments)` pair a
//!   `create_type_reference` interning was keyed on. It makes `T[]` against
//!   `number[]`, `Promise<T>` against `Promise<string>` and `C<T>` against
//!   `C<X>` decomposable, because *all* of them are references and `T[]` is
//!   `Array<T>` (`declared.rs`).
//! - [`Checker::signatures_of_type`] — `bd tsr-0hc`'s sibling index for
//!   function-shaped types. It makes `(x: T) => U` against `(x: number) =>
//!   string` decomposable.
//!
//! So the forecast implements four arms of `inferFromTypes` and **nothing
//! else**: the identity arm (`inference.go:1236`), the same-target
//! type-reference arm (`inferFromTypeArguments`, `inference.go:1046`), a
//! restricted union arm (`inferToMultipleTypes`, `inference.go:700`) and the
//! signature arm (`inferFromSignature`, `inference.go:1112`). No priority
//! lattice, no contravariant bucket, no intersection arm, no index signatures,
//! no mapped or conditional types, no tuples, no rest parameters. Every shape
//! outside those four arms contributes no candidate, which leaves its type
//! parameter unmapped, which makes the whole answer a gap — the same
//! discipline `inference.rs` already runs on.
//!
//! The restrictions on the union and signature arms, and the `null`/
//! `undefined` candidate refusal, were each **bought by a miss the probe
//! printed**; `docs/architecture/checker-notes-infer2.md` §2.2 records what
//! each cost in conversions.
//!
//! # Why the forecast is not a re-implementation of the answer
//!
//! Only the *candidate collection* is written here. The candidate is then
//! substituted through [`Checker::instantiate_type`] — the real one — and
//! printed through the real [`Checker::type_to_string`], so a forecast that
//! matches the baseline is a string the arm can actually produce. What the
//! probe adds is the one function that does not exist yet.
//!
//! # Own-node lines only
//!
//! Classification is restricted to lines whose **own node is the call**. A
//! variable initialised by such a call, and every read of it, is a *cascade*
//! line: its baseline text is the widened/declared form, not the call's, and
//! forecasting it would need the declaration-site transforms too. Those lines
//! are upside and must not enter a floor. `newgen.rs` took the same cut and
//! under-forecast its arm as a result.
//!
//! # Controls
//!
//! - **C1** every classified line answers `errorType` today. Expect 0
//!   violations; a non-zero reading means the classifier admitted a line that
//!   is not a gap and the whole table is inflated.
//! - **C2** the buckets sum to `classified`.
//! - **C3** the *bare-only* forecast — the rule `inference.rs` already ships,
//!   run through this same harness — must convert **0** of the classified
//!   lines. Every classified line is a gap today, so anything the existing
//!   rule can answer would already be answered; a non-zero reading means the
//!   probe's signature/parameter reconstruction diverges from
//!   `check_generic_call`'s and the CONVERTS bucket is measuring the harness
//!   rather than the mechanism. This is the control pinned to the *construct*
//!   rather than to arithmetic (`docs/conventions.md`).

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId};
use tsr_checker::{Checker, signatures::Signature, types::TypeId};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    converting_cases: BTreeMap<String, usize>,
    misses: BTreeMap<String, usize>,
    unmapped: BTreeMap<String, usize>,
    unrebuildable: BTreeMap<String, usize>,
    c1_not_gap: usize,
    c3_bare_converts: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        self.c3_bare_converts += other.c3_bare_converts;
        for (k, n) in &other.forms {
            *self.forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.converting_cases {
            *self.converting_cases.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.misses {
            *self.misses.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.unmapped {
            *self.unmapped.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.unrebuildable {
            *self.unrebuildable.entry(k.clone()).or_default() += n;
        }
    }
}

/// The candidate walk being sized: `inferFromTypes` (`inference.go:53`) over
/// the two structured reverse indices this port has.
///
/// Pushes `(type parameter, candidate)` for every match found. Contributes
/// nothing — deliberately — for every shape outside the three arms.
fn infer_from_types(
    checker: &Checker<'_, '_>,
    source: TypeId,
    target: TypeId,
    parameters: &[TypeId],
    out: &mut Vec<(TypeId, TypeId)>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    // Arm 1, `inference.go:1236`: the target *is* a type parameter.
    if parameters.contains(&target) {
        out.push((target, source));
        return;
    }
    // Arm 2, `inferFromTypeArguments` (`inference.go:1046`): two references to
    // the same target, argument for argument. Variance is not consulted —
    // upstream picks covariant/contravariant/invariant per position and this
    // slice treats every position covariantly, which is why a disagreement
    // between two positions has to gap rather than pick.
    let target_reference = checker.type_reference_target(target).cloned();
    let source_reference = checker.type_reference_target(source).cloned();
    if let (Some((ts, ta)), Some((ss, sa))) = (target_reference, source_reference) {
        if ts == ss && ta.len() == sa.len() {
            for (t, s) in ta.iter().zip(sa.iter()) {
                infer_from_types(checker, *s, *t, parameters, out, depth + 1);
            }
        }
        return;
    }
    // Arm 3, `inferToMultipleTypes` (`inference.go:700`), reduced to its
    // no-naked-type-variable leg: with no constituent that *is* a type
    // parameter, upstream infers the source into every constituent in turn,
    // and the constituents that cannot match contribute nothing. A union that
    // *does* carry a naked type variable — `T | undefined` — is refused: that
    // is the leg where upstream strikes the matched constituents and unions
    // whatever source remains, which needs `get_union_type` and a subtype
    // decision this port cannot make.
    //
    // `p.then(f)` is the head case: `then`'s parameter is
    // `((value: T) => ...) | null | undefined`, no constituent is a bare type
    // parameter, and only the signature constituent matches the argument.
    if let tsr_checker::types::TypeData::Union { types, .. } = &checker.type_of(target).data {
        let constituents = types.clone();
        // At most one naked type variable, and a source that is not itself a
        // union. Both restrictions are upstream's hard cases: two naked
        // variables is `inferToMultipleTypes`' ambiguous branch, and a union
        // source needs the matched constituents struck and the remainder
        // re-unioned, which needs `get_union_type` and a subtype decision.
        if constituents.iter().filter(|t| parameters.contains(t)).count() > 1 {
            return;
        }
        if matches!(&checker.type_of(source).data, tsr_checker::types::TypeData::Union { .. }) {
            return;
        }
        // `Promise<void>` against `TResult1 | PromiseLike<TResult1>`: upstream
        // matches the *reference* constituent through `Promise`'s base type
        // and infers `void`. This port has no base-type walk, so the naked
        // variable would swallow the whole `Promise<void>` and print
        // `Promise<Promise<void>>`. Refuse whenever a reference source faces a
        // reference constituent it does not share a symbol with.
        if let Some((source_symbol, _)) = checker.type_reference_target(source).cloned()
            && constituents.iter().any(|c| {
                checker.type_reference_target(*c).is_some_and(|(s, _)| *s != source_symbol)
            })
        {
            return;
        }
        for constituent in constituents {
            infer_from_types(checker, source, constituent, parameters, out, depth + 1);
        }
        return;
    }
    // Arm 4, `inferFromSignature` (`inference.go:1112`): parameter for
    // parameter and then the return type. A signature carrying its own type
    // parameters is refused — upstream erases them first
    // (`getErasedSignature`), which this port has no route to.
    let target_signatures = checker.signatures_of_type(target).cloned();
    let source_signatures = checker.signatures_of_type(source).cloned();
    if let (Some(t), Some(s)) = (target_signatures, source_signatures) {
        let ([t], [s]) = (t.as_slice(), s.as_slice()) else { return };
        if !t.type_parameters.is_empty() || !s.type_parameters.is_empty() {
            return;
        }
        if t.parameters.iter().chain(&s.parameters).any(|p| p.rest) {
            return;
        }
        // `applyToParameterTypes` (`inference.go:1140`) pairs positions up to
        // the shorter list: a callback declared with fewer parameters than the
        // target accepts is legal TypeScript and is the common shape —
        // `p.then(() => 1)` against `(value: T) => …`.
        for (tp, sp) in t.parameters.iter().zip(s.parameters.iter()) {
            infer_from_types(checker, sp.r#type, tp.r#type, parameters, out, depth + 1);
        }
        infer_from_types(checker, s.r#type, t.r#type, parameters, out, depth + 1);
    }
}

/// A coarse description of a type for the refusal split: which of the walk's
/// three arms *could* have applied, plus the printed form so the head of each
/// bucket is readable.
fn shape_of(checker: &Checker<'_, '_>, id: TypeId) -> String {
    let text = checker.type_to_string(id);
    if checker.type_reference_target(id).is_some() {
        return format!("reference `{text}`");
    }
    if checker.signatures_of_type(id).is_some() {
        return format!("signature `{text}`");
    }
    match &checker.type_of(id).data {
        tsr_checker::types::TypeData::Union { .. } => format!("union `{text}`"),
        tsr_checker::types::TypeData::Intersection { .. } => format!("intersection `{text}`"),
        tsr_checker::types::TypeData::Anonymous { .. } => format!("anonymous `{text}`"),
        tsr_checker::types::TypeData::Named { .. } => format!("named `{text}`"),
        _ => format!("other `{text}`"),
    }
}

/// The rule `inference.rs` ships today, expressed in the same shape as
/// [`infer_from_types`] so control C3 compares like with like: a candidate only
/// where the parameter's type *is* the type parameter.
fn infer_bare_only(
    source: TypeId,
    target: TypeId,
    parameters: &[TypeId],
    out: &mut Vec<(TypeId, TypeId)>,
) {
    if parameters.contains(&target) {
        out.push((target, source));
    }
}

/// The type each of a signature's own type parameters declares, in order —
/// `inference.rs`'s `type_parameter_types`, which is private to that crate.
fn type_parameter_types(
    checker: &mut Checker<'_, '_>,
    binder: &tsr_binder::BindResult<'_>,
    map: &tsr_ast::NodeMap<'_>,
    signature: &Signature,
) -> Option<Vec<TypeId>> {
    let declarations = match map.get(signature.declaration)? {
        Node::FunctionDeclaration(node) => node.type_parameters,
        Node::FunctionExpression(node) => node.type_parameters,
        Node::ArrowFunction(node) => node.type_parameters,
        Node::MethodDeclaration(node) => node.type_parameters,
        Node::MethodSignatureDeclaration(node) => node.type_parameters,
        Node::CallSignatureDeclaration(node) => node.type_parameters,
        Node::ConstructSignatureDeclaration(node) => node.type_parameters,
        Node::FunctionTypeNode(node) => node.type_parameters,
        _ => return None,
    };
    let symbols = declarations
        .iter()
        .map(|declaration| declaration.node_id.and_then(|id| binder.symbol_of(id)))
        .collect::<Option<Vec<_>>>()?;
    Some(symbols.into_iter().map(|symbol| checker.get_declared_type_of_symbol(symbol)).collect())
}

/// Run one candidate rule to a printed forecast, or `Err(reason)`.
#[allow(clippy::too_many_arguments)]
fn forecast(
    checker: &mut Checker<'_, '_>,
    signature: &Signature,
    parameters: &[TypeId],
    names: &[&str],
    argument_types: &[TypeId],
    structural: bool,
) -> Result<String, String> {
    let error = checker.intrinsics().error;
    let mut candidates: Vec<(TypeId, TypeId)> = Vec::new();
    for (index, parameter) in signature.parameters.iter().enumerate() {
        let Some(&argument) = argument_types.get(index) else { continue };
        if structural {
            infer_from_types(checker, argument, parameter.r#type, parameters, &mut candidates, 0);
        } else {
            infer_bare_only(argument, parameter.r#type, parameters, &mut candidates);
        }
    }
    let mut map: Vec<(TypeId, TypeId)> = Vec::new();
    for &type_parameter in parameters {
        let mut found: Option<TypeId> = None;
        for &(from, image) in &candidates {
            if from != type_parameter {
                continue;
            }
            match found {
                Some(previous) if previous != image => {
                    return Err("two candidates disagree for one type parameter".to_string());
                }
                _ => found = Some(image),
            }
        }
        // A `null` or `undefined` candidate is where `getWidenedType`
        // (`checker.go:16090`) decides the answer: with `strictNullChecks` off
        // upstream widens it to `any`, and nothing on this port's inference
        // path widens. Refuse rather than print `Promise<null>` where upstream
        // prints `Promise<any>`.
        if let Some(image) = found
            && matches!(checker.type_to_string(image).as_str(), "null" | "undefined")
        {
            return Err("a `null` or `undefined` candidate — needs `getWidenedType`".to_string());
        }
        match found {
            Some(image) if image != error => map.push((type_parameter, image)),
            Some(_) => return Err("a candidate is itself a gap".to_string()),
            None => {
                // Which *parameter position* the walk failed on, described by
                // the shape of the target and of the source. This is the split
                // that decides whether a wider slice exists.
                let mut where_ = String::from("(no parameter mentions it)");
                for (index, parameter) in signature.parameters.iter().enumerate() {
                    let Some(&argument) = argument_types.get(index) else { continue };
                    let mut probe = Vec::new();
                    infer_from_types(
                        checker,
                        argument,
                        parameter.r#type,
                        &[type_parameter],
                        &mut probe,
                        0,
                    );
                    if !probe.is_empty() {
                        continue;
                    }
                    let target = shape_of(checker, parameter.r#type);
                    let source = shape_of(checker, argument);
                    if target == "no mention" {
                        continue;
                    }
                    where_ = format!("target {target} :: source {source}");
                    break;
                }
                return Err(format!("no candidate — {where_}"));
            }
        }
    }
    let answer = checker.instantiate_type(signature.r#type, &map, parameters, names);
    if answer == error {
        return Err(format!("not rebuildable — return {}", shape_of(checker, signature.r#type)));
    }
    Ok(checker.type_to_string(answer))
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = Checker::with_module_host(bound, nodes, map, Some(&program));
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
            if want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                continue;
            }
            if got.type_string != "error" {
                continue;
            }
            let id: NodeId = line_ids[position];
            // Own-node lines only; see the module docs.
            let Some(Node::CallExpression(node)) = map.get(id) else { continue };
            if !node.type_arguments.is_empty() {
                continue;
            }
            if node.question_dot_token.is_some() {
                continue;
            }
            let Some(callee) = node.expression else { continue };
            let callee_type = checker.check_expression(callee);
            if callee_type == error {
                continue;
            }
            // Exactly the route `check_call_expression` takes, so the
            // classified population is `callgate.rs`'s
            // `single candidate, generic` gate and not a syntactic lookalike.
            let Some(signature) = checker.resolve_call_signature(callee_type, Some(node.arguments))
            else {
                continue;
            };
            if signature.type_parameters.is_empty() {
                continue;
            }
            let Some(parameters) = type_parameter_types(&mut checker, bound, map, &signature)
            else {
                continue;
            };
            if parameters.len() != signature.type_parameters.len() {
                continue;
            }
            let names: Vec<&str> =
                signature.type_parameters.iter().map(|p| p.name.as_str()).collect();

            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            *report.cases.entry(case.name.clone()).or_default() += 1;
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();

            let mut argument_types = Vec::with_capacity(node.arguments.len());
            let mut spread = false;
            for argument in node.arguments {
                if matches!(argument, tsr_ast::Expression::SpreadElement(_)) {
                    spread = true;
                }
                argument_types.push(checker.check_expression(*argument));
            }

            // C3: the shipped rule, through this same harness.
            if !spread
                && let Ok(bare) =
                    forecast(&mut checker, &signature, &parameters, &names, &argument_types, false)
                && bare == wanted
            {
                report.c3_bare_converts += 1;
            }

            let form = if spread {
                "a spread argument — no position to land on".to_string()
            } else if signature.parameters.iter().any(|p| p.rest) {
                "a rest parameter — `getSpreadArgumentType`, unported".to_string()
            } else if argument_types.contains(&error) {
                "an argument's own type is a gap".to_string()
            } else if signature.r#type == error {
                "the signature's return type is a gap".to_string()
            } else {
                match forecast(&mut checker, &signature, &parameters, &names, &argument_types, true)
                {
                    Err(reason) => {
                        if let Some(rest) = reason.strip_prefix("no candidate — ") {
                            *report.unmapped.entry(rest.to_string()).or_default() += 1;
                            "REFUSED — no candidate — the type parameter stays unmapped".to_string()
                        } else if let Some(rest) = reason.strip_prefix("not rebuildable — ") {
                            *report.unrebuildable.entry(rest.to_string()).or_default() += 1;
                            "REFUSED — the return type is not rebuildable by `instantiate_type`"
                                .to_string()
                        } else {
                            format!("REFUSED — {reason}")
                        }
                    }
                    Ok(f) if f == wanted => {
                        *report.converting_cases.entry(case.name.clone()).or_default() += 1;
                        "CONVERTS — forecast matches the baseline exactly".to_string()
                    }
                    Ok(_) if wanted == "any" => "want `any` (ceiling)".to_string(),
                    Ok(f) => {
                        *report
                            .misses
                            .entry(format!("want `{wanted}`, forecast `{f}`"))
                            .or_default() += 1;
                        "MISS — forecast differs".to_string()
                    }
                }
            };
            *report.forms.entry(form).or_default() += 1;
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

    println!("# infergen — the structural-inference counterfactual (bd tsr-g30h)\n");
    println!(
        "classified (own-node call, one generic candidate, gap line): {}\n",
        report.classified
    );
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C1 classified-but-not-gap : {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);
    println!("  C3 bare-only rule converts: {}  (expect 0)", report.c3_bare_converts);

    println!("\n## The misses, verbatim — what a forecast got wrong\n");
    let mut misses: Vec<_> = report.misses.iter().collect();
    misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (miss, n) in misses.into_iter().take(20) {
        println!("  {n:>5}  {miss}");
    }

    println!("\n## The no-candidate refusals, by where the walk stopped\n");
    let mut rows: Vec<_> = report.unmapped.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(25) {
        println!("  {n:>5}  {row}");
    }

    println!("\n## The not-rebuildable refusals, by return-type shape\n");
    let mut rows: Vec<_> = report.unrebuildable.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(15) {
        println!("  {n:>5}  {row}");
    }

    println!("\n## Top cases, all classified\n");
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }

    println!("\n## Top cases, CONVERTS only\n");
    let mut cases: Vec<_> = report.converting_cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
