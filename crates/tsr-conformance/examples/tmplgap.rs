//! Sizing probe for the `TemplateExpression` row — `STATUS.md` §5 refuses it
//! at 1,036 lines on the grounds that *"the cheap leg is not separable —
//! upstream's `evaluate` is a syntactic folder consulting no types"*.
//! `depend.rs` at HEAD reads the row at **1,890 lines, want-any 8 (0.4%)**,
//! so the refusal is being re-tested on a fresh number as the rules require.
//!
//! `checkTemplateExpression` (`checker.go:7976`) has exactly four outcomes,
//! in order:
//!
//! 1. `evaluate(node)` folds the whole template to a constant string —
//!    answer a **fresh string literal** (`checker.go:7995`);
//! 2. a `const` context, a template-literal context, or a template-literal
//!    contextual type — answer a **template literal type**;
//! 3. otherwise — answer **`string`** (`checker.go:8000`).
//!
//! The refusal's premise is that (1) cannot be told from (3) without the
//! folder. This probe tests that by partitioning the row on **what the
//! baseline wants**, which is the only evidence that decides it:
//!
//! - `string` exactly — reachable by the cheap leg alone;
//! - a **string literal** whose value this probe folds syntactically — the
//!   `evaluate` leg, and the probe folds it here so the leg's size is
//!   measured rather than assumed;
//! - a literal this probe could NOT fold — names the folder's real cost;
//! - anything else (a template literal type, a union) — out of scope.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Expression, Node};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    unfoldable: BTreeMap<String, usize>,
    c1_not_gap: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        for (k, n) in &other.forms {
            *self.forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.unfoldable {
            *self.unfoldable.entry(k.clone()).or_default() += n;
        }
    }
}

/// The syntactic half of upstream's `evaluate` for a template expression:
/// fold to a constant string when every span's expression is itself a
/// literal. Deliberately syntax-only, which is what upstream's own folder is.
/// `None` means "this probe cannot fold it" — reported as its own bucket so
/// the folder's real cost is measured rather than assumed.
fn fold(expression: &tsr_ast::TemplateExpression<'_>) -> Option<String> {
    let mut out = String::new();
    out.push_str(expression.head?.text);
    for span in expression.template_spans {
        let value = match span.expression? {
            Expression::StringLiteral(literal) => literal.text.to_string(),
            Expression::NoSubstitutionTemplateLiteral(literal) => literal.text.to_string(),
            Expression::NumericLiteral(literal) => {
                tsr_checker::printing::normalise_number(literal.text)
            }
            _ => return None,
        };
        out.push_str(&value);
        out.push_str(match span.literal? {
            tsr_ast::TemplateMiddleOrTail::TemplateMiddle(m) => m.text,
            tsr_ast::TemplateMiddleOrTail::TemplateTail(t) => t.text,
        });
    }
    Some(out)
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
            if want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                continue;
            }
            if got.type_string != "error" {
                continue;
            }
            let id = line_ids[position];
            let Some(Node::TemplateExpression(expression)) = map.get(id) else { continue };
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();
            let folded = fold(expression);

            let form = if wanted == "string" {
                // The cheap leg. Whether the folder would ALSO have fired here
                // is the separability question, so it is split rather than
                // merged: a line wanting `string` that this probe folds to a
                // literal would be a line the cheap leg gets right and the
                // folder gets wrong.
                match &folded {
                    Some(_) => "want `string`, and the folder WOULD fold it (conflict)".to_string(),
                    None => "want `string`, unfoldable — the cheap leg alone".to_string(),
                }
            } else if wanted.starts_with('"') || wanted.starts_with('`') {
                let literal = wanted.trim_matches('"').to_string();
                match &folded {
                    Some(value) if *value == literal => {
                        "want a literal, and this probe folds it EXACTLY".to_string()
                    }
                    Some(value) => {
                        *report
                            .unfoldable
                            .entry(format!("want `{wanted}`, folded `{value}`"))
                            .or_default() += 1;
                        "want a literal, folded DIFFERENTLY".to_string()
                    }
                    None => {
                        *report
                            .unfoldable
                            .entry(format!("want `{wanted}`, NOT foldable"))
                            .or_default() += 1;
                        "want a literal, this probe cannot fold it".to_string()
                    }
                }
            } else if wanted == "any" {
                "want `any` (ceiling)".to_string()
            } else {
                *report.unfoldable.entry(format!("want `{wanted}` (other)")).or_default() += 1;
                "want something else — template literal type or union".to_string()
            };
            *report.forms.entry(form).or_default() += 1;
            *report.cases.entry(case.name.clone()).or_default() += 1;
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

    println!("# tmplgap — the TemplateExpression row, split by what the baseline wants\n");
    println!("classified: {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);

    println!("\n## What the folder could not reach, verbatim\n");
    let mut misses: Vec<_> = report.unfoldable.iter().collect();
    misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (miss, n) in misses.into_iter().take(15) {
        println!("  {n:>5}  {miss}");
    }

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(8) {
        println!("  {n:>6}  {case}");
    }
}
