//! How much upstream prints from the **written annotation** — `bd tsr-a2c`.
//!
//! # The mechanism, and the reading of it I got wrong twice
//!
//! `tsr-a2c` was filed from one lost line: `unionAndIntersectionInference3`
//! prints `>args : (Maybe<T>[] | Maybe<T>)[]` — sorted — and the enclosing
//! signature as `<T>(...args: (Maybe<T> | Maybe<T>[])[]) => T[]` — the order
//! written in source. Same parameter, same type, two renderings in one case.
//!
//! `symbolToParameterDeclaration` (`nodebuilderimpl.go:1654`) builds a
//! parameter's type with `serializeTypeForDeclaration(…, tryReuse: true)`
//! (`:2181`), whose reuse branch (`:2229`) is gated on
//! `ast.HasInferredType(declaration)`.
//!
//! **That gate reads like "the declaration has no annotation" and is not.**
//! `HasInferredType` (`ast/utilities.go:4100`) is a **node-kind** test:
//! `KindParameter`, `KindPropertySignature`, `KindPropertyDeclaration`,
//! `KindVariableDeclaration` and friends return `true` unconditionally. So the
//! branch *is* taken for an annotated parameter, and what actually decides
//! reuse is `pseudoTypeEquivalentToType(pt, t, …)` — is the written annotation
//! equivalent to the computed type? If yes, **the written node is reused and
//! printed verbatim.**
//!
//! I read that gate the wrong way while sizing this and corrected it before
//! writing it down. It is recorded because the wrong reading is the natural one
//! and the next person will make it: the function is named for what it is
//! *used* to decide, not for what it tests.
//!
//! # What this measures
//!
//! An **aligned wrong line** whose node is a declaration name, where the
//! baseline's right-hand side is exactly the **source text of that
//! declaration's own type annotation**, whitespace-normalised. That is the
//! signature of upstream having reused the written node while this port printed
//! its computed type.
//!
//! It is a **lower bound on the mechanism and an upper bound on nothing**:
//! reuse also fires inside signatures, where the reused node is one parameter of
//! a larger rendering and no single annotation equals the whole line. Those are
//! counted separately and only as a population.
//!
//! # Controls
//!
//! - **C1, construction.** A line whose baseline already equals our answer
//!   cannot be in the population — it is not a wrong line. Printed as the
//!   `right today` column, which must be 0 by the loop's own `continue`.
//! - **C2, construction.** A declaration with **no** annotation cannot supply
//!   source text, so it cannot enter the matched bucket. Counted, and it is the
//!   bucket that says how much of the wrong-declaration population this
//!   mechanism *cannot* explain.

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeTable};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Upstream renders a declaration's type with `serializeTypeForDeclaration`,
/// which reuses the written node. These are the node kinds whose *name* is an
/// assertion line and whose declaration carries an annotation.
fn annotation_of(node: Node<'_>) -> Option<tsr_ast::TypeNode<'_>> {
    match node {
        Node::VariableDeclaration(d) => d.r#type,
        Node::ParameterDeclaration(d) => d.r#type,
        Node::PropertySignatureDeclaration(d) => d.r#type,
        Node::PropertyDeclaration(d) => d.r#type,
        _ => None,
    }
}

/// The declaration a rendered line's node is the *name* of.
fn declaration_of<'a>(
    nodes: &NodeTable,
    map: &tsr_ast::NodeMap<'a>,
    id: NodeId,
) -> Option<Node<'a>> {
    let parent = nodes.parent(id)?;
    let node = map.get(parent)?;
    (node.name_id() == Some(id)).then_some(node)
}

/// Upstream's printer normalises whitespace inside a type; the source may carry
/// newlines and runs of spaces. Comparing on a single-spaced form is the
/// closest exact test available without re-implementing the emitter.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Default)]
struct Report {
    /// `(bucket) -> lines`.
    buckets: BTreeMap<&'static str, usize>,
    cases: BTreeMap<&'static str, HashMap<String, usize>>,
    /// The exact `ours -> annotation` pairs where reuse would convert.
    pairs: BTreeMap<(String, String), usize>,
    /// Wrong lines whose answer renders a signature — where reuse fires *inside*
    /// the rendering and no single annotation equals the line. Population only.
    inside_signature: usize,
    c1_right_today: usize,
    /// Spans that do not address this unit's text. `NodeTable` is
    /// program-wide, so a span is only meaningful against its own file, and a
    /// mismatch is a **skip that must be printed** rather than a silent
    /// under-count — the first run of this probe panicked on one.
    span_unusable: usize,
    /// **The number that decides the item.** Lines that are RIGHT today, whose
    /// declaration carries an annotation, and whose written text differs from
    /// what this port prints. Printing the annotation would break every one.
    /// `docs/conventions.md`: an accuracy bar on the target row licenses
    /// nothing when the mechanism fires wider.
    at_risk: usize,
    /// Right today and the written text already equals what we print — the
    /// mechanism is a no-op for these.
    right_and_agrees: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        for (k, n) in &other.buckets {
            *self.buckets.entry(k).or_default() += n;
        }
        for (k, cases) in &other.cases {
            let mine = self.cases.entry(k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        for (k, n) in &other.pairs {
            *self.pairs.entry(k.clone()).or_default() += n;
        }
        self.inside_signature += other.inside_signature;
        self.c1_right_today += other.c1_right_today;
        self.span_unusable += other.span_unusable;
        self.at_risk += other.at_risk;
        self.right_and_agrees += other.right_and_agrees;
    }

    fn add(&mut self, bucket: &'static str, case: &str) {
        *self.buckets.entry(bucket).or_default() += 1;
        *self.cases.entry(bucket).or_default().entry(case.to_owned()).or_default() += 1;
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

    let mut report = Report::default();
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        // The unit is looked up **by name**, not by index: the rendered
        // sections and the case's `files` do not line up once a lib unit is
        // in play, and an index would silently slice the wrong text.
        let Some(unit) = parsed.files.iter().find(|f| f.name == expected_file.file) else {
            continue;
        };
        let source = unit.content.as_str();
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            if want.text == got.line() {
                // The wider population: what a reuse rule would also fire on.
                if let Some(declaration) = declaration_of(nodes, map, line_ids[position])
                    && let Some(annotation) = annotation_of(declaration)
                    && let Some(annotation_id) = annotation.node_id()
                {
                    let span = nodes.span(annotation_id);
                    let (start, end) = (span.start as usize, span.end as usize);
                    if start < end && end <= source.len() {
                        if normalise(&source[start..end]) == normalise(&got.type_string) {
                            report.right_and_agrees += 1;
                        } else {
                            report.at_risk += 1;
                        }
                    }
                }
                continue;
            }
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if got.type_string == "error" {
                continue;
            }
            let id = line_ids[position];
            let Some(declaration) = declaration_of(nodes, map, id) else {
                if got.type_string.contains("=>") {
                    report.inside_signature += 1;
                }
                continue;
            };
            let Some(annotation) = annotation_of(declaration) else {
                report.add("the declaration has NO annotation", &case.name);
                continue;
            };
            let Some(annotation_id) = annotation.node_id() else { continue };
            let span = nodes.span(annotation_id);
            let (start, end) = (span.start as usize, span.end as usize);
            if start >= end || end > source.len() {
                report.span_unusable += 1;
                continue;
            }
            let written = normalise(&source[start..end]);
            if written == normalise(want_type) {
                report.add("baseline == the WRITTEN annotation (reuse would convert)", &case.name);
                *report.pairs.entry((got.type_string.clone(), written)).or_default() += 1;
            } else {
                report.add("baseline differs from the written annotation too", &case.name);
            }
        }
    }
    Some(report)
}

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

    println!("# reuse — how much upstream prints from the written annotation (`bd tsr-a2c`)\n");
    println!("Population: aligned WRONG lines whose node is a declaration name.\n");
    let mut rows: Vec<_> = report.buckets.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (bucket, n) in rows {
        let empty = HashMap::new();
        let cases = report.cases.get(bucket).unwrap_or(&empty);
        let (top, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
        #[allow(clippy::cast_precision_loss)]
        let share = *top_n as f64 / (*n).max(1) as f64 * 100.0;
        println!("  {bucket:<48} {n:>6}   {:>5} cases, top-1 {share:>5.1}%  {top}", cases.len());
    }
    println!(
        "\n  THE WIDER POPULATION — lines that are RIGHT today and carry an annotation:\n    the written text already equals what we print (no-op): {}\n    the written text DIFFERS — printing it would break these: {}",
        report.right_and_agrees, report.at_risk
    );
    println!(
        "\n  spans that did not address the unit's own text, skipped: {}",
        report.span_unusable
    );
    println!(
        "  C1 (construction): a line that matches the baseline never enters — {} counted",
        report.c1_right_today
    );
    println!(
        "  C2 (construction): the no-annotation bucket above is what this mechanism cannot explain"
    );
    println!(
        "\n  wrong lines rendering a SIGNATURE, where reuse fires inside the rendering and no\n  single annotation equals the line — POPULATION only, no conversion: {}",
        report.inside_signature
    );

    println!("\n  the exact `ours -> written annotation` pairs that reuse would convert:\n");
    let mut pairs: Vec<_> = report.pairs.iter().collect();
    pairs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((ours, written), n) in pairs.iter().take(15) {
        println!("      {n:>5}  {ours}\n             -> {written}");
    }
}
