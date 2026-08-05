//! What the `QualifiedName` identifier population actually answers.
//!
//! `cargo run -p tsr-conformance --example qualified_name_left --release`
//!
//! The item was sized at **4,577 wrong `Identifier` lines whose parent is a
//! `QualifiedName`**, 97.3% of them with a `TypeReference` grandparent, and the
//! substitution was never measured — only the population. This probe measures
//! the substitution, and splits it by the structural position of the identifier
//! within the qualified name, which is what decides which upstream rule applies.
//!
//! # Arms
//!
//! | arm | the node |
//! |---|---|
//! | `LeftInTypeQuery` | the `M` of `typeof M.C` — keeps its **value** type |
//! | `LeftInTypeRef` | the `M` of `let x: M.I` — upstream prints `any` |
//! | `LeftOther` | left of a qualified name under neither |
//! | `Right` | the `I` of `M.I` — the type name itself |
//!
//! Every arm prints its **positive control**: how many aligned lines fall in it
//! at all, right or wrong. An arm reading zero wrong is unreadable without it.
//! The **control bucket** — a `QualifiedName`-parent identifier matching no arm —
//! is printed unconditionally, including at zero; it can only be non-zero if the
//! four arms above do not partition the position space, which would mean the
//! model is wrong.
//!
//! # What it found at `f99072c`: the item is 174 lines, not 4,577
//!
//! ```text
//! arm                                        control    right    wrong      gap
//! left of qualified name, under `typeof`         186       70        5      111
//! left of qualified name, under a TypeReference 5266     5266        0        0
//! left of qualified name, other grandparent      173       40      133        0
//! right of qualified name (the type name)        508        0       36      472
//! UNATTRIBUTED (control bucket)                    0
//! ```
//!
//! **The 4,455-line `TypeReference` arm is already zero-wrong.** `f6524eb` and
//! `cb173ea` — the `any` rule and its `typeof` exemption in
//! `types_producer::type_at_location` — closed it, and the 4,577 figure was taken
//! before them. The item as sized no longer exists; what survives is 174 wrong
//! lines, and **133 of them are one rule**.
//!
//! ## The 133: the entity name of an `import x = a.b`
//!
//! Every `LeftOther` wrong line has the same enclosing kind — 133 wrong and 34
//! right under `ImportEqualsDeclaration`, nothing else non-zero — and every one
//! of them is `ours any` against `upstream typeof {name}`. The producer's `any`
//! rule exempts only `TypeQuery`, so it over-applies here.
//!
//! Upstream does **not** reach this through `IsExpressionNode`: its
//! `KindQualifiedName` arm walks to the outermost qualified name and asks
//! `IsTypeQueryNode || IsJSDocLinkLike || IsJSDocNameReference || IsJsxTagName`,
//! all false for an import-equals. It falls through `getTypeOfNode` past the type
//! node, expression, class, type-declaration, binding and declaration branches to
//! `isInRightSideOfImportOrExportAssignment`, which takes
//! `getDeclaredTypeOfSymbol` and falls back to `getTypeOfSymbol` when that is the
//! error type. For a namespace `a` the declared type *is* the error type, so the
//! answer is `getTypeOfSymbol` — `typeof a`. That is exactly the observed
//! substitution, and it is a third exemption alongside `TypeQuery`, in the same
//! producer rule.
//!
//! The 34 right lines under the same kind are the ones where upstream also says
//! `any`, so the exemption cannot be unconditional — it has to reproduce the
//! declared-then-value fallback rather than always answering the value type.
//!
//! ## The remaining 41 are not one thing
//!
//! 36 on the **right** of a qualified name (against 472 gaps in the same arm —
//! the right side is overwhelmingly a *gap* population, not a wrong one) and 5
//! under `typeof`, whose substitutions are all distinct shapes. Neither is a
//! rule.
//!
//! `QUALIFIED_NAME_PROBE_FLATTEN=1` is the mutation: it collapses `LeftInTypeQuery`
//! into `LeftInTypeRef`, so the two arms' lines can be watched merging rather
//! than being silently redistributed. Verified: under it `LeftInTypeQuery` reads
//! 0/0/0/0 and `LeftInTypeRef` reads 5452/5336/5/111 — the 186/70/5/111 moved
//! across intact, with the 6,133 population and the 174 wrong unchanged.

use std::collections::HashMap;

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Arm {
    LeftInTypeQuery,
    LeftInTypeRef,
    LeftOther,
    Right,
}

impl Arm {
    const ORDER: [Arm; 4] = [Arm::LeftInTypeQuery, Arm::LeftInTypeRef, Arm::LeftOther, Arm::Right];

    fn label(self) -> &'static str {
        match self {
            Arm::LeftInTypeQuery => "left of qualified name, under `typeof`",
            Arm::LeftInTypeRef => "left of qualified name, under a TypeReference",
            Arm::LeftOther => "left of qualified name, other grandparent",
            Arm::Right => "right of qualified name (the type name itself)",
        }
    }
}

/// Which arm this identifier falls in, or `None` if its parent is not a
/// `QualifiedName` at all.
fn arm_of(nodes: &NodeTable, map: &NodeMap, id: NodeId) -> Option<Arm> {
    let parent = nodes.parent(id)?;
    if nodes.kind(parent) != SyntaxKind::QualifiedName {
        return None;
    }
    let Some(Node::QualifiedName(qualified)) = map.get(parent) else { return None };
    if qualified.right.and_then(|right| right.node_id) == Some(id) {
        return Some(Arm::Right);
    }

    // The walk is up through **nested** qualified names: `typeof A.B.C` nests
    // them and only the outermost parent is the `TypeQuery`. Same walk the
    // producer does, deliberately — this probe measures the producer's own rule.
    let mut outermost = parent;
    while let Some(above) = nodes.parent(outermost) {
        if nodes.kind(above) != SyntaxKind::QualifiedName {
            break;
        }
        outermost = above;
    }
    let flatten = std::env::var("QUALIFIED_NAME_PROBE_FLATTEN").is_ok();
    Some(match nodes.parent(outermost).map(|above| nodes.kind(above)) {
        Some(SyntaxKind::TypeQuery) if !flatten => Arm::LeftInTypeQuery,
        // `TypeQuery` reaches here only under the mutation, which is the point:
        // it must land in the same arm as `TypeReference`, not vanish.
        Some(SyntaxKind::TypeQuery | SyntaxKind::TypeReference) => Arm::LeftInTypeRef,
        _ => Arm::LeftOther,
    })
}

#[derive(Default)]
struct Tally {
    cases: usize,
    aligned: usize,
    /// The gradient itself, over **every** aligned line of any kind — the only
    /// number that can say whether a rule is a net gain. An arm's own right/wrong
    /// split cannot: this rule moves lines out of `wrong` into `gap` as well as
    /// into `right`, and perturbs unrelated lines through the checker's caches.
    aligned_right: usize,
    aligned_gap: usize,
    /// Positive control: every aligned line in the arm, right or wrong.
    control: HashMap<Arm, usize>,
    right: HashMap<Arm, usize>,
    wrong: HashMap<Arm, usize>,
    gap: HashMap<Arm, usize>,
    /// `(arm, ours -> theirs)` for the wrong lines.
    subst: HashMap<(Arm, String, String), usize>,
    /// A `QualifiedName`-parent identifier the four arms failed to place.
    unattributed: usize,
    /// Wrong `Identifier` lines whose parent is **not** a `QualifiedName`, so
    /// the arm totals can be read against the whole wrong-identifier pool.
    wrong_identifiers_elsewhere: usize,
    /// For `LeftOther`: the kind of the node above the outermost qualified name,
    /// which is what the arm failed to recognise. Split right/wrong, because an
    /// enclosing kind that is always right is not the one to fix.
    other_enclosing: HashMap<(String, bool), usize>,
}

impl Tally {
    fn merge(&mut self, other: Tally) {
        self.cases += other.cases;
        self.aligned += other.aligned;
        self.aligned_right += other.aligned_right;
        self.aligned_gap += other.aligned_gap;
        self.unattributed += other.unattributed;
        self.wrong_identifiers_elsewhere += other.wrong_identifiers_elsewhere;
        for (key, value) in other.control {
            *self.control.entry(key).or_default() += value;
        }
        for (key, value) in other.right {
            *self.right.entry(key).or_default() += value;
        }
        for (key, value) in other.wrong {
            *self.wrong.entry(key).or_default() += value;
        }
        for (key, value) in other.gap {
            *self.gap.entry(key).or_default() += value;
        }
        for (key, value) in other.subst {
            *self.subst.entry(key).or_default() += value;
        }
        for (key, value) in other.other_enclosing {
            *self.other_enclosing.entry(key).or_default() += value;
        }
    }
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let total = cases
        .par_iter()
        .map(|case| {
            let mut tally = Tally::default();
            if case.has_varied_types() || case.has_known_divergence() {
                return tally;
            }
            let Some(text) = case.expected_types() else { return tally };
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return tally;
            }
            let Ok(parsed) = case.load() else { return tally };
            tally.cases = 1;

            for expected_file in &expected {
                let Some(unit) = parsed.files.iter().find(|u| {
                    tsr_conformance::binder_suite::same_unit(&u.name, &expected_file.file)
                }) else {
                    continue;
                };
                if tsr_parser::ScriptKind::from_file_name(&unit.name)
                    == tsr_parser::ScriptKind::Json
                {
                    continue;
                }
                let arena = tsr_core::Arena::new();
                let options = tsr_parser::ParseOptions {
                    jsdoc: false,
                    ..tsr_parser::ParseOptions::for_file(&unit.name)
                };
                let file = tsr_parser::parse_with_options(&arena, &unit.content, options);
                let bound = tsr_binder::bind(
                    file.source_file,
                    &file.nodes,
                    tsr_binder::FileInfo { name: &unit.name, text: &unit.content },
                );
                let mut checker = tsr_checker::Checker::new(&bound, &file.nodes, &file.node_map);
                let mut ids = Vec::new();
                let rendered = types_producer::assertions_for_file(
                    &Node::SourceFile(file.source_file),
                    &unit.content,
                    &file.nodes,
                    &file.node_map,
                    |id| {
                        ids.push(id);
                        types_producer::type_at_location(
                            &mut checker,
                            &bound,
                            &file.nodes,
                            &file.node_map,
                            id,
                        )
                    },
                );
                assert_eq!(ids.len(), rendered.len(), "one recorded id per rendered line");

                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = rendered.get(position) else { continue };
                    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    tally.aligned += 1;
                    if want_type == got.type_string {
                        tally.aligned_right += 1;
                    } else if got.type_string == "error" {
                        tally.aligned_gap += 1;
                    }
                    if got.kind != SyntaxKind::Identifier {
                        continue;
                    }
                    let parent_is_qualified = nodes_parent_is_qualified(&file.nodes, ids[position]);
                    let arm = arm_of(&file.nodes, &file.node_map, ids[position]);
                    match arm {
                        Some(arm) => *tally.control.entry(arm).or_default() += 1,
                        // The control bucket: parent IS a `QualifiedName` and yet
                        // no arm placed it. The four arms are meant to partition
                        // the position space, so this can only be non-zero if the
                        // model is wrong.
                        None if parent_is_qualified => tally.unattributed += 1,
                        None => {}
                    }
                    let correct = want_type == got.type_string;
                    let Some(arm) = arm else {
                        if !correct && got.type_string != "error" {
                            tally.wrong_identifiers_elsewhere += 1;
                        }
                        continue;
                    };
                    if arm == Arm::LeftOther {
                        *tally
                            .other_enclosing
                            .entry((enclosing_kind(&file.nodes, ids[position]), correct))
                            .or_default() += 1;
                    }
                    if correct {
                        *tally.right.entry(arm).or_default() += 1;
                    } else if got.type_string == "error" {
                        *tally.gap.entry(arm).or_default() += 1;
                    } else {
                        *tally.wrong.entry(arm).or_default() += 1;
                        *tally
                            .subst
                            .entry((arm, got.type_string.clone(), want_type.to_string()))
                            .or_default() += 1;
                    }
                }
            }
            tally
        })
        .reduce(Tally::default, |mut a, b| {
            a.merge(b);
            a
        });

    report(&total);
}

/// Kept separate from [`arm_of`] so the control bucket asks a question the arm
/// logic cannot answer for itself.
/// The kind of the node above the outermost enclosing `QualifiedName` — the
/// thing the `LeftInTypeQuery`/`LeftInTypeRef` test is asking about.
fn enclosing_kind(nodes: &NodeTable, id: NodeId) -> String {
    let Some(mut outermost) = nodes.parent(id) else { return "<none>".to_string() };
    while let Some(above) = nodes.parent(outermost) {
        if nodes.kind(above) != SyntaxKind::QualifiedName {
            break;
        }
        outermost = above;
    }
    match nodes.parent(outermost) {
        Some(above) => format!("{:?}", nodes.kind(above)),
        None => "<none>".to_string(),
    }
}

fn nodes_parent_is_qualified(nodes: &NodeTable, id: NodeId) -> bool {
    nodes.parent(id).is_some_and(|parent| nodes.kind(parent) == SyntaxKind::QualifiedName)
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 }
}

fn report(total: &Tally) {
    println!("cases judged          {:>9}", total.cases);
    println!("aligned lines         {:>9}", total.aligned);
    println!(
        "  of which RIGHT      {:>9}   {:.4}%  <- the gradient; the only net number",
        total.aligned_right,
        pct(total.aligned_right, total.aligned)
    );
    println!(
        "  of which gap        {:>9}   wrong {}",
        total.aligned_gap,
        total.aligned - total.aligned_right - total.aligned_gap
    );
    println!();
    println!(
        "{:<52} {:>8} {:>8} {:>8} {:>8}",
        "arm (positive control = every aligned line in it)", "control", "right", "wrong", "gap"
    );
    let mut population = 0;
    let mut wrong_total = 0;
    for arm in Arm::ORDER {
        let control = total.control.get(&arm).copied().unwrap_or(0);
        let right = total.right.get(&arm).copied().unwrap_or(0);
        let wrong = total.wrong.get(&arm).copied().unwrap_or(0);
        let gap = total.gap.get(&arm).copied().unwrap_or(0);
        population += control;
        wrong_total += wrong;
        println!("{:<52} {control:>8} {right:>8} {wrong:>8} {gap:>8}", arm.label());
    }
    println!("{:<52} {:>8}", "UNATTRIBUTED (control bucket)", total.unattributed);
    println!();
    println!(
        "QualifiedName-parent identifier lines  {population:>8}   wrong {wrong_total} ({:.2}%)",
        pct(wrong_total, population)
    );
    println!("wrong Identifier lines with any OTHER parent  {}", total.wrong_identifiers_elsewhere);
    println!();

    println!("--- LeftOther, by the kind enclosing the outermost QualifiedName ---");
    let mut rows: Vec<_> =
        total.other_enclosing.iter().map(|((k, ok), n)| (*n, k.clone(), *ok)).collect();
    rows.sort_by_key(|(n, _, _)| std::cmp::Reverse(*n));
    for (n, kind, ok) in &rows {
        println!("  {n:>6}  {kind:<34} {}", if *ok { "right" } else { "WRONG" });
    }
    println!();

    for arm in Arm::ORDER {
        let wrong = total.wrong.get(&arm).copied().unwrap_or(0);
        println!("--- {} --- wrong {wrong}", arm.label());
        let mut rows: Vec<_> = total
            .subst
            .iter()
            .filter(|((a, _, _), _)| *a == arm)
            .map(|((_, ours, theirs), n)| (*n, ours.clone(), theirs.clone()))
            .collect();
        rows.sort_by_key(|(n, _, _)| std::cmp::Reverse(*n));
        for (n, ours, theirs) in rows.iter().take(12) {
            println!("  {n:>6}  ours {ours:<34} upstream {theirs}");
        }
        if rows.is_empty() {
            println!("  (none)");
        }
        println!();
    }
}
