//! Feasibility probe for the JSX element row — STATUS.md §4.2's 230-score
//! item. `checker-notes-jsx.md` sizes the element lines (want `JSX.Element`
//! 1,194, `any` 477, `error` 85) and names the mechanism:
//! `getJsxElementTypeAt` (`jsx.go`) — resolve the `JSX` namespace in scope,
//! read its `Element` interface, answer that type. What nobody has measured
//! is whether **this port** can already walk that path: resolve `JSX`,
//! find `Element` in its exports, compute a declared type, and print the
//! string the baseline wants.
//!
//! For every gap line whose node is a `JsxElement`, `JsxSelfClosingElement`
//! or `JsxFragment`, walk the path and bucket where it stops. The printed
//! text matters as much as the resolution: upstream prints `JSX.Element`,
//! and if the declared type prints bare `Element` the item owns a naming
//! problem (`bd tsr-93f`'s family) on every converted line.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::Node;
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        for (k, n) in &other.forms {
            *self.forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
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
            if !matches!(
                map.get(id),
                Some(Node::JsxElement(_) | Node::JsxSelfClosingElement(_) | Node::JsxFragment(_))
            ) {
                continue;
            }
            report.classified += 1;
            let wanted = want.text.rsplit_once(" : ").map_or("?", |(_, answer)| answer).to_string();
            let want_bucket = if wanted == "JSX.Element" {
                "JSX.Element"
            } else if wanted == "any" {
                "any"
            } else if wanted == "error" {
                "error"
            } else {
                "other"
            };

            // The mechanism's path, one hop at a time.
            let step = if let Some(jsx) =
                bound.resolve_name(nodes, map, id, "JSX", SymbolFlags::NAMESPACE)
            {
                if let Some(&element) = bound.symbols().get(jsx).exports.get("Element") {
                    let declared = checker.get_declared_type_of_symbol(element);
                    if declared == error {
                        "JSX.Element resolves, its declared type GAPS".to_string()
                    } else {
                        format!("declared type prints `{}`", checker.type_to_string(declared))
                    }
                } else {
                    "JSX resolves, no `Element` export".to_string()
                }
            } else {
                "`JSX` does not resolve as a namespace".to_string()
            };

            let form = format!("want {want_bucket:<12} | {step}");
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

    println!("# jsxfeas — can this port already walk getJsxElementTypeAt's path?\n");
    println!("classified JSX element/fragment gap lines: {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (form, n) in rows {
        println!("  {n:>6}  {form}");
    }
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(8) {
        println!("  {n:>6}  {case}");
    }
}
