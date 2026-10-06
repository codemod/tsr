//! **The composite-print seam, priced symbol-exactly** (`bd tsr-2ghn`).
//!
//! ```text
//! cargo run -p tsr-conformance --release --example sigprint
//! ```
//!
//! A function-shaped type's text is baked at creation, so a `Named` type
//! embedded in its return slot renders site-lessly — `() => Element` where the
//! baseline wants `() => JSX.Element`. The token-rewrite model of this family
//! (`qualnamep.rs`'s loose section) selects candidates **by name** and its
//! at-risk column is dominated by name collisions the real mechanism cannot
//! make (`bd tsr-2ghn`, second refinement). This probe models the **faithful**
//! design instead, for the *return slot only*:
//!
//! for every aligned line whose type carries signature structure
//! ([`tsr_checker::Checker::signatures_of_type`], kept by `bd tsr-0hc`), take
//! the single signature's return [`TypeId`], render it **at the line's site**
//! (`type_to_string_at` — the shipped naming stack: qualifiers, renames,
//! refusals), and forecast the printed text with the baked return substring
//! replaced. Symbol-exact by construction: the `TypeId` is the type the
//! signature actually returns, so no name collision is possible.
//!
//! The three columns are the standard ones. AT-RISK is the design's real cost
//! and the number the token model could not produce honestly.

use std::collections::BTreeMap;

use rayon::prelude::*;
use tsr_checker::signatures::{Signature, SignatureKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// `Checker::signature_to_string` (`signatures.rs:1435`), replicated with a
/// `site`: every rendered slot goes through `type_to_string_at` when a site is
/// given, falling back to the baked text where the site-aware path refuses.
/// The written-text and predicate precedence rules are the composer's own;
/// predicate signatures are skipped by the caller (the predicate printer is
/// crate-private and its text is site-independent anyway).
/// The braces form — `{ (x: T): R; (y: U): S; }` — the member spelling
/// (`symbols.rs:1309`, `objects.rs::signature_member_text`), site-aware.
fn compose_braces(
    checker: &mut tsr_checker::Checker<'_, '_>,
    signatures: &[Signature],
    site: Option<tsr_ast::NodeId>,
) -> String {
    let mut out = String::from("{ ");
    for signature in signatures {
        let arrow = compose(checker, signature, site);
        // Member spelling: the arrow form with `) => R` re-spelled `): R`,
        // and Construct/AbstractConstruct both print bare `new `.
        let arrow = arrow.replace("abstract new ", "new ");
        match arrow.rfind(") => ") {
            Some(at) => {
                out.push_str(&arrow[..=at]);
                out.push_str(": ");
                out.push_str(&arrow[at + 5..]);
            }
            None => out.push_str(&arrow),
        }
        out.push_str("; ");
    }
    out.push('}');
    out
}

fn compose(
    checker: &mut tsr_checker::Checker<'_, '_>,
    signature: &Signature,
    site: Option<tsr_ast::NodeId>,
) -> String {
    fn render(
        checker: &mut tsr_checker::Checker<'_, '_>,
        id: tsr_checker::TypeId,
        site: Option<tsr_ast::NodeId>,
    ) -> String {
        match site {
            Some(site) => {
                checker.type_to_string_at(id, site).unwrap_or_else(|| checker.type_to_string(id))
            }
            None => checker.type_to_string(id),
        }
    }
    let mut out = match signature.kind {
        SignatureKind::Call => String::new(),
        SignatureKind::Construct => "new ".to_string(),
        SignatureKind::AbstractConstruct => "abstract new ".to_string(),
    };
    if !signature.type_parameters.is_empty() {
        out.push('<');
        for (index, parameter) in signature.type_parameters.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&parameter.name);
            if let Some(constraint) = parameter.constraint {
                out.push_str(" extends ");
                if let Some(written) = &parameter.written_constraint {
                    out.push_str(written);
                } else {
                    let text = render(checker, constraint, site);
                    out.push_str(&text);
                }
            }
            if let Some(default) = parameter.default {
                out.push_str(" = ");
                let text = render(checker, default, site);
                out.push_str(&text);
            }
        }
        out.push('>');
    }
    out.push('(');
    for (index, parameter) in
        signature.this_parameter.iter().chain(signature.parameters.iter()).enumerate()
    {
        if index > 0 {
            out.push_str(", ");
        }
        if parameter.rest {
            out.push_str("...");
        }
        out.push_str(&parameter.name);
        out.push_str(if parameter.optional { "?: " } else { ": " });
        if let Some(written) = &parameter.written_text {
            out.push_str(written);
        } else {
            let parameter_type = checker.parameter_type(parameter);
            let text = render(checker, parameter_type, site);
            out.push_str(&text);
        }
    }
    out.push_str(") => ");
    if let Some(written) = &signature.written_return {
        out.push_str(written);
    } else {
        let text = render(checker, signature.r#type, site);
        out.push_str(&text);
    }
    out
}

#[derive(Default)]
struct Report {
    /// Lines whose type has exactly one signature and whose baked text ends
    /// with the baked return rendering — the population the forecast can act on.
    admitted: usize,
    converts: usize,
    churn: usize,
    at_risk: usize,
    gap_return: usize,
    /// Wrong-today lines whose type carries 2+ signatures — the overload
    /// shard's population ceiling, not a forecast.
    multi_signature_wrong: usize,
    braces_converts: usize,
    braces_churn: usize,
    braces_at_risk: usize,
    braces_self_check_miss: usize,
    lines: BTreeMap<String, usize>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.admitted += other.admitted;
        self.converts += other.converts;
        self.churn += other.churn;
        self.at_risk += other.at_risk;
        self.gap_return += other.gap_return;
        self.multi_signature_wrong += other.multi_signature_wrong;
        self.braces_converts += other.braces_converts;
        self.braces_churn += other.braces_churn;
        self.braces_at_risk += other.braces_at_risk;
        self.braces_self_check_miss += other.braces_self_check_miss;
        for (key, n) in &other.lines {
            *self.lines.entry(key.clone()).or_default() += n;
        }
    }
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
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));

    let mut report = Report::default();
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            let id = line_ids[position];
            let is_right = want.text == got.line();
            if !is_right && want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                continue;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();
            let printed = got.type_string.clone();
            if printed == "error" {
                continue;
            }
            let type_id = types_producer::type_id_at_location(&mut checker, bound, nodes, map, id);
            let Some(signatures) = checker.signatures_of_type(type_id) else { continue };
            // The single-signature arrow/function shape only: the overload and
            // construct forms bake differently and are a later shard — its
            // wrong-today population tallied here (reusing `at_risk`... no:
            // counted in `churn`-adjacent field below).
            if signatures.len() != 1 {
                if !is_right {
                    report.multi_signature_wrong += 1;
                }
                // The braces-form shard: self-check, then forecast.
                let signatures = signatures.clone();
                if signatures.iter().any(|s| s.predicate.is_some()) {
                    continue;
                }
                let rebuilt = compose_braces(&mut checker, &signatures, None);
                if rebuilt != printed {
                    report.braces_self_check_miss += 1;
                    continue;
                }
                let forecast = compose_braces(&mut checker, &signatures, Some(id));
                if forecast == printed {
                    continue;
                }
                let outcome = if is_right {
                    report.braces_at_risk += 1;
                    "B-AT RISK"
                } else if forecast == wanted {
                    report.braces_converts += 1;
                    "B-CONVERTS"
                } else {
                    report.braces_churn += 1;
                    "B-WOULD-WRONG"
                };
                *report
                    .lines
                    .entry(format!(
                        "{outcome:<13} want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                        case.name
                    ))
                    .or_default() += 1;
                continue;
            }
            let signature = signatures[0].clone();
            if signature.predicate.is_some() {
                continue;
            }
            // SELF-CHECK: the site-less reconstruction must equal what the
            // compiler printed, or the format model here is not the
            // composer's and no forecast below is readable.
            let rebuilt = compose(&mut checker, &signature, None);
            if rebuilt != printed {
                report.gap_return += 1; // reused as the self-check-miss counter
                continue;
            }
            report.admitted += 1;
            let forecast = compose(&mut checker, &signature, Some(id));
            let outcome = if forecast == printed {
                continue;
            } else if is_right {
                report.at_risk += 1;
                "AT RISK"
            } else if forecast == wanted {
                report.converts += 1;
                "CONVERTS"
            } else {
                report.churn += 1;
                "WOULD-WRONG"
            };
            *report
                .lines
                .entry(format!(
                    "{outcome:<11} want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                    case.name
                ))
                .or_default() += 1;
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
    println!("# sigprint — the return slot of the composite-print seam, symbol-exact\n");
    println!("  admitted (1 signature, self-check passed)            {:>7}", report.admitted);
    println!("  CONVERTS      wrong today, forecast IS the want      {:>7}", report.converts);
    println!("  WOULD-WRONG   wrong today, still not the want        {:>7}", report.churn);
    println!("  AT RISK       right today, forecast changes it       {:>7}", report.at_risk);
    println!("  (self-check misses — format model != composer: {})", report.gap_return);
    println!(
        "  multi-signature wrong-today lines (overload shard ceiling): {}",
        report.multi_signature_wrong
    );
    println!(
        "  BRACES shard: converts {} / would-wrong {} / at-risk {} / self-check misses {}",
        report.braces_converts,
        report.braces_churn,
        report.braces_at_risk,
        report.braces_self_check_miss
    );
    println!();
    let mut rows: Vec<_> = report.lines.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (line, n) in rows.iter().take(40) {
        let line = if line.len() > 160 { &line[..160] } else { line };
        println!("  {n:>5}  {line}");
    }
}
