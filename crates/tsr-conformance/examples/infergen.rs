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
    c5_shipped_converts_a_gap: usize,
    delta_changed: usize,
    delta_converts: usize,
    delta_wrong: usize,
    delta_to_gap: usize,
    delta_wrong_forms: BTreeMap<String, usize>,
    right_classified: usize,
    right_c4_divergence: usize,
    right_at_risk: usize,
    right_at_risk_forms: BTreeMap<String, usize>,
    right_c4_conservative: usize,
    right_c4_disagrees: usize,
    wrong_classified: usize,
    wrong_fixed: usize,
    wrong_fixed_forms: BTreeMap<String, usize>,
    wrong_cases: BTreeMap<String, usize>,
    forms_wantany: BTreeMap<String, usize>,
    forms_cases: BTreeMap<String, BTreeMap<String, usize>>,
    lattice_changed: usize,
    lattice_converts: usize,
    lattice_wrong: usize,
    lattice_wrong_forms: BTreeMap<String, usize>,
    right_at_risk_lattice: usize,
    solo_changed: usize,
    solo_converts: usize,
    solo_wrong: usize,
    solo_wrong_forms: BTreeMap<String, usize>,
    solo_cases: BTreeMap<String, usize>,
    right_at_risk_solo: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        self.c3_bare_converts += other.c3_bare_converts;
        self.c5_shipped_converts_a_gap += other.c5_shipped_converts_a_gap;
        self.delta_changed += other.delta_changed;
        self.delta_converts += other.delta_converts;
        self.delta_wrong += other.delta_wrong;
        self.delta_to_gap += other.delta_to_gap;
        self.right_classified += other.right_classified;
        self.right_c4_divergence += other.right_c4_divergence;
        self.right_at_risk += other.right_at_risk;
        self.right_c4_conservative += other.right_c4_conservative;
        self.right_c4_disagrees += other.right_c4_disagrees;
        self.wrong_classified += other.wrong_classified;
        self.wrong_fixed += other.wrong_fixed;
        self.lattice_changed += other.lattice_changed;
        self.right_at_risk_lattice += other.right_at_risk_lattice;
        self.solo_changed += other.solo_changed;
        self.solo_converts += other.solo_converts;
        self.solo_wrong += other.solo_wrong;
        self.right_at_risk_solo += other.right_at_risk_solo;
        for (k, n) in &other.solo_wrong_forms {
            *self.solo_wrong_forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.solo_cases {
            *self.solo_cases.entry(k.clone()).or_default() += n;
        }
        self.lattice_converts += other.lattice_converts;
        self.lattice_wrong += other.lattice_wrong;
        for (k, n) in &other.lattice_wrong_forms {
            *self.lattice_wrong_forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.wrong_fixed_forms {
            *self.wrong_fixed_forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.wrong_cases {
            *self.wrong_cases.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.forms_wantany {
            *self.forms_wantany.entry(k.clone()).or_default() += n;
        }
        for (k, inner) in &other.forms_cases {
            let mine = self.forms_cases.entry(k.clone()).or_default();
            for (c, n) in inner {
                *mine.entry(c.clone()).or_default() += n;
            }
        }
        for (k, n) in &other.delta_wrong_forms {
            *self.delta_wrong_forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.right_at_risk_forms {
            *self.right_at_risk_forms.entry(k.clone()).or_default() += n;
        }
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
    checker: &mut Checker<'_, '_>,
    source: TypeId,
    target: TypeId,
    parameters: &[TypeId],
    out: &mut Vec<(TypeId, TypeId, bool)>,
    depth: usize,
    contravariant: bool,
) {
    if depth > 16 {
        return;
    }
    // Arm 1, `inference.go:1236`: the target *is* a type parameter.
    if parameters.contains(&target) {
        out.push((target, source, contravariant));
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
                infer_from_types(checker, *s, *t, parameters, out, depth + 1, contravariant);
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
        // Parity with the shipped arm (`inference.rs`): strike the
        // constituents the source already matches before anything reaches the
        // naked type variable, excluding `never`/`any` sources which are
        // assignable to everything.
        let source_is_wildcard =
            source == checker.intrinsics().never || source == checker.intrinsics().any;
        if !source_is_wildcard
            && constituents
                .iter()
                .any(|&c| !parameters.contains(&c) && checker.is_type_assignable_to(source, c))
        {
            return;
        }
        for constituent in constituents {
            infer_from_types(
                checker,
                source,
                constituent,
                parameters,
                out,
                depth + 1,
                contravariant,
            );
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
            let source_parameter = checker.parameter_type(sp);
            let target_parameter = checker.parameter_type(tp);
            infer_from_types(
                checker,
                source_parameter,
                target_parameter,
                parameters,
                out,
                depth + 1,
                !contravariant,
            );
        }
        infer_from_types(checker, s.r#type, t.r#type, parameters, out, depth + 1, contravariant);
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
    out: &mut Vec<(TypeId, TypeId, bool)>,
) {
    if parameters.contains(&target) {
        out.push((target, source, false));
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
#[allow(
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    reason = "each flag names one upstream rule this probe runs with and without"
)]
fn forecast(
    checker: &mut Checker<'_, '_>,
    signature: &Signature,
    parameters: &[TypeId],
    names: &[&str],
    argument_types: &[TypeId],
    structural: bool,
    strict_null_checks: bool,
    consult_contra: bool,
    common_supertype: bool,
) -> Result<String, String> {
    let error = checker.intrinsics().error;
    let mut candidates: Vec<(TypeId, TypeId, bool)> = Vec::new();
    for (index, parameter) in signature.parameters.iter().enumerate() {
        let Some(&argument) = argument_types.get(index) else { continue };
        let parameter_type = checker.parameter_type(parameter);
        if structural {
            infer_from_types(
                checker,
                argument,
                parameter_type,
                parameters,
                &mut candidates,
                0,
                false,
            );
        } else {
            infer_bare_only(argument, parameter_type, parameters, &mut candidates);
        }
    }
    let never = checker.intrinsics().never;
    let any = checker.intrinsics().any;
    let mut map: Vec<(TypeId, TypeId)> = Vec::new();
    for &type_parameter in parameters {
        // One bucket each, `inferFromContravariantTypes` (`inference.go:308`).
        // `contravariant` is only *collected* here; whether it is consulted is
        // the whole question this probe sizes.
        // With `consult_contra` OFF there is **one** bucket, not an ignored
        // second one: the shipped arm collects a candidate found through a
        // function argument's parameter position into the same set as a bare
        // one, which is precisely why the two disagree and gap
        // (`checker-notes-infer2.md` §5.2). Running the same walk both ways is
        // what makes every column below a **delta attributable to this
        // mechanism** rather than a difference between the probe and the
        // compiler.
        let agreed = |checker: &mut Checker<'_, '_>, contra: bool| -> Result<Option<TypeId>, ()> {
            let mine: Vec<TypeId> = candidates
                .iter()
                .filter(|&&(from, _, c)| from == type_parameter && (!consult_contra || c == contra))
                .map(|&(_, image, _)| image)
                .collect();
            let has_other = mine.iter().any(|&image| image != never);
            let kept: Vec<TypeId> =
                mine.into_iter().filter(|&image| !(has_other && image == never)).collect();
            let mut found = None;
            let mut disagreed = false;
            for &image in &kept {
                match found {
                    Some(previous) if previous != image => {
                        disagreed = true;
                        break;
                    }
                    _ => found = Some(image),
                }
            }
            if !disagreed {
                return Ok(found);
            }
            if !common_supertype {
                return Err(());
            }
            // `getCovariantInference` (`inference.go:1434`) resolves a
            // disagreement one of two ways: a subtype-reduced union under
            // `InferencePriorityPriorityImpliesCombination`, else
            // `getCommonSupertype` (`inference.go:1530`), whose rule is *the
            // leftmost type for which no type to the right is a supertype*.
            // The union branch needs `UnionReductionSubtype`, refused at
            // `bd tsr-eak`. The supertype branch is expressible here in its
            // unambiguous case — exactly one candidate that every other is
            // assignable to — and that is the leg being sized. Neither
            // `getCommonSupertype`'s literal-types-with-a-common-base branch
            // nor `getCovariantInference`'s literal widening is modelled.
            let mut best = None;
            for &image in &kept {
                if kept.iter().all(|&other| checker.is_type_assignable_to(other, image)) {
                    if best.is_some_and(|b| b != image) {
                        return Err(());
                    }
                    best = Some(image);
                }
            }
            match best {
                Some(image) => Ok(Some(image)),
                None => Err(()),
            }
        };
        let Ok(covariant) = agreed(checker, false) else {
            return Err("two covariant candidates disagree".to_string());
        };
        // With `consult_contra` off the contravariant bucket is *collected and
        // discarded*, which is exactly the resolution rule `inference.rs` ships
        // today (`checker-notes-infer2.md` §3: the existing resolution rules are
        // kept). Running the same walk both ways is what makes every column
        // below a **delta attributable to this mechanism** rather than a
        // difference between the probe and the compiler.
        let contra: Vec<TypeId> = if consult_contra {
            candidates
                .iter()
                .filter(|&&(from, _, c)| from == type_parameter && c)
                .map(|&(_, image, _)| image)
                .collect()
        } else {
            Vec::new()
        };
        // `getInferredType` (`inference.go:1317`): prefer the covariant
        // inference when it is not `never`/`any` and **some** contravariant
        // candidate is a supertype of it; otherwise prefer the contravariant
        // one. With no contravariant candidates the covariant one wins
        // outright, which is what ships today.
        let found = match (covariant, contra.is_empty()) {
            (Some(c), true) => Some(c),
            (Some(c), false) => {
                let prefer = c != never
                    && c != any
                    && contra.iter().any(|&t| checker.is_type_assignable_to(c, t));
                if prefer {
                    Some(c)
                } else {
                    let Ok(v) = agreed(checker, true) else {
                        return Err("contravariant candidates disagree — needs `getCommonSubtype`"
                            .to_string());
                    };
                    v
                }
            }
            (None, false) => {
                let Ok(v) = agreed(checker, true) else {
                    return Err(
                        "contravariant candidates disagree — needs `getCommonSubtype`".to_string()
                    );
                };
                v
            }
            (None, true) => None,
        };
        // `getWidenedType` (`checker.go:18355`) widens a `null`/`undefined`
        // inference to `any` only with `strictNullChecks` off; nothing here
        // widens, so that case is refused.
        if let Some(image) = found
            && !strict_null_checks
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
                    let parameter_type = checker.parameter_type(parameter);
                    let mut probe = Vec::new();
                    infer_from_types(
                        checker,
                        argument,
                        parameter_type,
                        &[type_parameter],
                        &mut probe,
                        0,
                        false,
                    );
                    if !probe.is_empty() {
                        continue;
                    }
                    let target = shape_of(checker, parameter_type);
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = Checker::with_module_host(bound, nodes, map, Some(&program));
    // The same two directives `types_producer` reads, with the same `true`
    // default: a probe checking under different strictness than the gradient
    // would forecast a different compiler.
    let explicit = |name: &str| parsed.options.get(name).map(|v| v.eq_ignore_ascii_case("true"));
    let strict_null_checks =
        explicit("strictnullchecks").or_else(|| explicit("strict")).unwrap_or(true);
    checker.set_strict_null_checks(strict_null_checks);
    let error = checker.intrinsics().error;

    let mut report = Report::default();
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            // `docs/conventions.md`: a mechanism that fires on a POSITION —
            // and the contravariant bucket fires on every generic call, not on
            // a defect — has its at-risk column computed in the SAME pass as
            // its target one. So a line that is RIGHT today is admitted here
            // rather than skipped, and is classified below.
            let is_right = want.text == got.line();
            let mut is_wrong = false;
            if !is_right {
                if want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                    continue;
                }
                // A line that is WRONG today is the third side of the ledger
                // §6.3 quoted by hand at the merge commit. It is re-taken here
                // by the same instrument, because design W and design P landed
                // between then and now and both moved the wrong bucket.
                is_wrong = got.type_string != "error";
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

            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();

            let mut argument_types = Vec::with_capacity(node.arguments.len());
            let mut spread = false;
            for argument in node.arguments {
                if matches!(argument, tsr_ast::Expression::SpreadElement(_)) {
                    spread = true;
                }
                argument_types.push(checker.check_expression(*argument));
            }

            if is_wrong {
                report.wrong_classified += 1;
                *report.wrong_cases.entry(case.name.clone()).or_default() += 1;
                if spread || signature.parameters.iter().any(|p| p.rest) {
                    continue;
                }
                let shipped = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    false,
                    false,
                );
                let proposed = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    true,
                    false,
                );
                if shipped != proposed && proposed.as_deref() == Ok(wanted.as_str()) {
                    report.wrong_fixed += 1;
                    *report.wrong_fixed_forms.entry(case.name.clone()).or_default() += 1;
                }
                continue;
            }

            if is_right {
                // AT RISK — lines RIGHT today whose printed text this mechanism
                // would change. Measured as a delta between the same walk run
                // with one candidate bucket and with two, so a probe that
                // simply models the arm imperfectly cannot inflate the column.
                report.right_classified += 1;
                if spread || signature.parameters.iter().any(|p| p.rest) {
                    continue;
                }
                let shipped = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    false,
                    false,
                );
                if shipped.as_deref() != Ok(wanted.as_str()) {
                    if shipped.is_err() {
                        report.right_c4_conservative += 1;
                    } else {
                        report.right_c4_disagrees += 1;
                    }
                    // C4: the line is right, so the compiler prints `wanted`.
                    // The probe failing to reproduce that is divergence, and it
                    // makes the at-risk column a LOWER bound rather than an
                    // upper one.
                    report.right_c4_divergence += 1;
                    continue;
                }
                let proposed = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    true,
                    false,
                );
                let lattice = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    true,
                    true,
                );
                if lattice.as_deref() != Ok(wanted.as_str()) {
                    report.right_at_risk_lattice += 1;
                }
                let solo = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    false,
                    true,
                );
                if solo.as_deref() != Ok(wanted.as_str()) {
                    report.right_at_risk_solo += 1;
                }
                if proposed.as_deref() != Ok(wanted.as_str()) {
                    report.right_at_risk += 1;
                    let became = match &proposed {
                        Ok(f) => format!("forecast `{f}`"),
                        Err(reason) => format!("GAP — {reason}"),
                    };
                    *report
                        .right_at_risk_forms
                        .entry(format!("{}: want `{wanted}`, {became}", case.name))
                        .or_default() += 1;
                }
                continue;
            }

            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            *report.cases.entry(case.name.clone()).or_default() += 1;

            // C3: the shipped rule, through this same harness.
            if !spread
                && let Ok(bare) = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    false,
                    strict_null_checks,
                    true,
                    false,
                )
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
                // The SHIPPED resolution rule through the same walk — one
                // candidate bucket. Everything below that says "delta" is
                // measured against this and not against the published §6.2
                // table, whose CONVERTS column could not tell a contravariant
                // conversion from a probe/compiler divergence.
                let shipped = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    false,
                    false,
                );
                let proposed = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    true,
                    false,
                );
                // The OTHER half of the row's named remainder: the priority
                // lattice. Its own population is the `two covariant candidates
                // disagree` bucket, and the only leg of `getCovariantInference`
                // this port can express is the common-supertype pick.
                let lattice = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    true,
                    true,
                );
                // The lattice leg ALONE — one candidate bucket, common-supertype
                // pick. This is the design that needs no `strictFunctionTypes`
                // default invented for it, which is the modelling guess §6.1
                // refused the contravariant bucket on.
                let solo = forecast(
                    &mut checker,
                    &signature,
                    &parameters,
                    &names,
                    &argument_types,
                    true,
                    strict_null_checks,
                    false,
                    true,
                );
                if solo != shipped {
                    report.solo_changed += 1;
                    match (&solo, wanted.as_str()) {
                        (Ok(f), w) if f == w => {
                            report.solo_converts += 1;
                            *report.solo_cases.entry(case.name.clone()).or_default() += 1;
                        }
                        (Ok(f), _) if wanted != "any" => {
                            *report
                                .solo_wrong_forms
                                .entry(format!("want `{wanted}`, forecast `{f}`"))
                                .or_default() += 1;
                            report.solo_wrong += 1;
                        }
                        _ => {}
                    }
                }
                if lattice != proposed {
                    report.lattice_changed += 1;
                    match (&lattice, wanted.as_str()) {
                        (Ok(f), w) if f == w => report.lattice_converts += 1,
                        (Ok(f), _) if wanted != "any" => {
                            *report
                                .lattice_wrong_forms
                                .entry(format!("want `{wanted}`, forecast `{f}`"))
                                .or_default() += 1;
                            report.lattice_wrong += 1;
                        }
                        _ => {}
                    }
                }
                if shipped.as_deref() == Ok(wanted.as_str()) {
                    // C5: every classified line is a gap today, so the shipped
                    // rule converting one means the probe and the compiler
                    // disagree — not that the mechanism gained anything.
                    report.c5_shipped_converts_a_gap += 1;
                }
                if shipped != proposed {
                    report.delta_changed += 1;
                    match (&proposed, wanted.as_str()) {
                        (Ok(f), w) if f == w => report.delta_converts += 1,
                        (Ok(_), "any") => {}
                        (Ok(f), _) => {
                            *report
                                .delta_wrong_forms
                                .entry(format!("want `{wanted}`, forecast `{f}`"))
                                .or_default() += 1;
                            report.delta_wrong += 1;
                        }
                        (Err(_), _) => report.delta_to_gap += 1,
                    }
                }
                match proposed {
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
            if wanted == "any" {
                *report.forms_wantany.entry(form.clone()).or_default() += 1;
            }
            *report
                .forms_cases
                .entry(form.clone())
                .or_default()
                .entry(case.name.clone())
                .or_default() += 1;
            *report.forms.entry(form).or_default() += 1;
        }
    }
    Some(report)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "an assertion-line count is far inside f64's exact range"
)]
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
        let want_any = report.forms_wantany.get(form).copied().unwrap_or(0);
        let top1 = report
            .forms_cases
            .get(form)
            .and_then(|m| m.iter().max_by_key(|(_, n)| **n))
            .map(|(c, k)| format!("{c} {k} = {:.1}%", 100.0 * *k as f64 / *n as f64))
            .unwrap_or_default();
        println!(
            "  {n:>6}  want-any {want_any:>4} ({:>4.1}%)  top-1 {top1}  {form}",
            100.0 * want_any as f64 / *n as f64
        );
    }
    println!("\n  C1 classified-but-not-gap : {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);
    println!("  C3 bare-only rule converts: {}  (expect 0)", report.c3_bare_converts);
    println!("  C5 shipped rule converts a gap: {}  (expect 0)", report.c5_shipped_converts_a_gap);

    println!("\n## THE THREE COLUMNS — the contravariant bucket as a DELTA\n");
    println!("  lines the mechanism changes at all : {:>6}", report.delta_changed);
    println!("  CONVERTS                           : {:>6}", report.delta_converts);
    println!("  WOULD PRINT WRONG                  : {:>6}", report.delta_wrong);
    println!("  gap -> a different gap             : {:>6}", report.delta_to_gap);
    println!();
    println!("  right lines admitted (same pass)   : {:>6}", report.right_classified);
    println!(
        "  C4 probe cannot reproduce a right  : {:>6}   ({} the probe GAPS, {} it answers differently)",
        report.right_c4_divergence, report.right_c4_conservative, report.right_c4_disagrees
    );
    println!("  AT RISK (contravariant bucket)     : {:>6}", report.right_at_risk);
    println!("  AT RISK (+ lattice cheap leg)      : {:>6}", report.right_at_risk_lattice);
    println!("  AT RISK (lattice leg ALONE)        : {:>6}", report.right_at_risk_solo);
    println!();
    println!("  wrong lines admitted (same pass)   : {:>6}", report.wrong_classified);
    println!("  WRONG LINES IT WOULD FIX           : {:>6}", report.wrong_fixed);
    println!(
        "  wrong lines, contravariant-named cases: {}",
        report
            .wrong_cases
            .iter()
            .filter(|(k, _)| k.contains("ontravariant") || k.contains("trictFunctionTypes"))
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let mut rows: Vec<_> = report.wrong_fixed_forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(10) {
        println!("      {n:>4}  {row}");
    }

    println!(
        "\n## The PRIORITY LATTICE cheap leg — common-supertype pick, delta over the contravariant build\n"
    );
    println!("  lines it changes  : {:>6}", report.lattice_changed);
    println!("  CONVERTS          : {:>6}", report.lattice_converts);
    println!("  WOULD PRINT WRONG : {:>6}", report.lattice_wrong);
    let mut rows: Vec<_> = report.lattice_wrong_forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(10) {
        println!("      {n:>4}  {row}");
    }

    println!(
        "\n## The LATTICE LEG ALONE — no contravariant bucket, no `strictFunctionTypes` guess\n"
    );
    println!("  lines it changes  : {:>6}", report.solo_changed);
    println!("  CONVERTS          : {:>6}", report.solo_converts);
    println!("  WOULD PRINT WRONG : {:>6}", report.solo_wrong);
    let mut rows: Vec<_> = report.solo_cases.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(10) {
        println!("      converts {n:>4}  {row}");
    }
    let mut rows: Vec<_> = report.solo_wrong_forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(10) {
        println!("      wrong    {n:>4}  {row}");
    }

    println!("\n## AT RISK, verbatim\n");
    let mut rows: Vec<_> = report.right_at_risk_forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(25) {
        println!("  {n:>5}  {row}");
    }

    println!("\n## WOULD PRINT WRONG, verbatim\n");
    let mut rows: Vec<_> = report.delta_wrong_forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (row, n) in rows.into_iter().take(25) {
        println!("  {n:>5}  {row}");
    }

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
