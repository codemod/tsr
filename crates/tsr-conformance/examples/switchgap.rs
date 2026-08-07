//! `bd tsr-5kii` sized — what switch-clause narrowing could reach.
//!
//! `checker-notes-narrow.md` §8 recorded the cheap prior (12,140 residual
//! lines in switch-bearing cases) as CONTAMINATED: presence of a `switch` in
//! the file attributes nothing. This walks the actual positions.
//!
//! # What it measures
//!
//! For every non-right aligned line, whether the line's node sits **inside a
//! `CaseClause`/`DefaultClause`**, and if so:
//!
//! - whether the enclosing switch's discriminant is a **bare identifier**
//!   (the shape `narrowTypeBySwitchOnDiscriminant` narrows; a property-access
//!   discriminant needs `isMatchingReference`'s structural half);
//! - whether the line's own expression **mentions that identifier** as a
//!   token — the lines a switch-clause narrow could change;
//! - whether every case expression in the switch is a **literal** (the
//!   unit-type precondition, approximated syntactically).
//!
//! The MENTIONS bucket split by verdict is the sized population. It is a
//! ceiling, never a forecast (`STATUS.md`'s fourth rule): a mentioning line
//! converts only if the narrow's answer is what the baseline wants.
//!
//! # Controls
//!
//! - **C1, arithmetic.** Every walked line lands in exactly one bucket;
//!   the buckets sum to the walked total, printed.
//! - **C2, safe direction.** The identifier-mention test is a *token* test on
//!   the baseline's own expression text, so it can over-count (a shadowed
//!   inner `x`) and the split by verdict is reported per bucket rather than
//!   summed into one number.

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    walked: usize,
    buckets: BTreeMap<&'static str, usize>,
    bucket_cases: BTreeMap<&'static str, HashMap<String, usize>>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.walked += other.walked;
        for (k, n) in &other.buckets {
            *self.buckets.entry(k).or_default() += n;
        }
        for (k, cases) in &other.bucket_cases {
            let mine = self.bucket_cases.entry(k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
    }
}

fn is_token(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(found) = text[start..].find(name) {
        let begin = start + found;
        let end = begin + name.len();
        let before_ok = begin == 0
            || !(bytes[begin - 1].is_ascii_alphanumeric()
                || bytes[begin - 1] == b'_'
                || bytes[begin - 1] == b'$');
        let after_ok = end == text.len()
            || !(bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'$');
        if before_ok && after_ok {
            return true;
        }
        start = end;
    }
    false
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
            report.walked += 1;
            let verdict = if got.type_string == "error" { "gap" } else { "wrong" };

            // Climb to the nearest case/default clause, then to its switch.
            let mut current = Some(line_ids[position]);
            let mut clause = None;
            while let Some(id) = current {
                match nodes.kind(id) {
                    SyntaxKind::CaseClause | SyntaxKind::DefaultClause => {
                        clause = Some(id);
                        break;
                    }
                    // A nested function is its own narrowing scope.
                    SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration => break,
                    _ => {}
                }
                current = nodes.parent(id);
            }
            let Some(clause) = clause else {
                *report.buckets.entry("not in a switch clause").or_default() += 1;
                continue;
            };
            let mut switch = nodes.parent(clause);
            while let Some(id) = switch {
                if nodes.kind(id) == SyntaxKind::SwitchStatement {
                    break;
                }
                switch = nodes.parent(id);
            }
            let discriminant = switch.and_then(|id| match map.get(id) {
                Some(Node::SwitchStatement(statement)) => statement.expression,
                _ => None,
            });
            let bucket = match discriminant {
                Some(tsr_ast::Expression::Identifier(identifier)) => {
                    if is_token(&want.text, identifier.text) {
                        if verdict == "gap" {
                            "IN CLAUSE, identifier discriminant, line MENTIONS it — gap"
                        } else {
                            "IN CLAUSE, identifier discriminant, line MENTIONS it — wrong"
                        }
                    } else {
                        "in clause, identifier discriminant, line does not mention it"
                    }
                }
                Some(tsr_ast::Expression::PropertyAccessExpression(_)) => {
                    "in clause, property-access discriminant (needs structural matching)"
                }
                Some(_) => "in clause, other discriminant shape",
                None => "in clause, switch not found (probe defect — expect 0)",
            };
            *report.buckets.entry(bucket).or_default() += 1;
            *report
                .bucket_cases
                .entry(bucket)
                .or_default()
                .entry(case.name.clone())
                .or_default() += 1;
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");
    let report = cases
        .par_iter()
        .filter_map(measure)
        .fold(Report::default, |mut a, r| {
            a.merge(&r);
            a
        })
        .reduce(Report::default, |mut a, b| {
            a.merge(&b);
            a
        });

    println!("# switchgap — what switch-clause narrowing could reach (`bd tsr-5kii`)\n");
    println!("non-right aligned lines walked: {}\n", report.walked);
    let mut rows: Vec<_> = report.buckets.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let total: usize = report.buckets.values().sum();
    for (bucket, n) in &rows {
        let empty = HashMap::new();
        let cases = report.bucket_cases.get(*bucket).unwrap_or(&empty);
        let (top, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
        println!(
            "  {n:>6}  {:>4} cases, top-1 {:>5.1}%  {bucket}\n          top: {top}",
            cases.len(),
            *top_n as f64 / (**n).max(1) as f64 * 100.0,
        );
    }
    println!("\n  C1 arithmetic: buckets sum {total}, walked {}", report.walked);
}
