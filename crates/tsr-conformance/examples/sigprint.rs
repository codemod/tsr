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
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    /// Lines whose type has exactly one signature and whose baked text ends
    /// with the baked return rendering — the population the forecast can act on.
    admitted: usize,
    converts: usize,
    churn: usize,
    at_risk: usize,
    gap_return: usize,
    lines: BTreeMap<String, usize>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.admitted += other.admitted;
        self.converts += other.converts;
        self.churn += other.churn;
        self.at_risk += other.at_risk;
        self.gap_return += other.gap_return;
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
            // construct forms bake differently and are a later shard.
            if signatures.len() != 1 {
                continue;
            }
            let return_type = signatures[0].r#type;
            let baked = checker.type_to_string(return_type);
            if baked == "error" {
                report.gap_return += 1;
                continue;
            }
            // The baked composite ends with the baked return rendering for the
            // `(…) => R` shape; anything else is out of this forecast's reach.
            if !printed.ends_with(&baked) {
                continue;
            }
            let site =
                checker.type_to_string_at(return_type, id).unwrap_or_else(|| "error".to_string());
            report.admitted += 1;
            if site == baked || site == "error" {
                continue;
            }
            let forecast = format!("{}{site}", &printed[..printed.len() - baked.len()]);
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
    println!("  admitted (1 signature, baked return is the suffix)   {:>7}", report.admitted);
    println!("  CONVERTS      wrong today, forecast IS the want      {:>7}", report.converts);
    println!("  WOULD-WRONG   wrong today, still not the want        {:>7}", report.churn);
    println!("  AT RISK       right today, forecast changes it       {:>7}", report.at_risk);
    println!("  (return type itself gaps: {} — no claim)", report.gap_return);
    println!();
    let mut rows: Vec<_> = report.lines.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (line, n) in rows.iter().take(40) {
        let line = if line.len() > 160 { &line[..160] } else { line };
        println!("  {n:>5}  {line}");
    }
}
