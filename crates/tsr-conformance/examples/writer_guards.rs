//! What each of the baseline writer's eight guards is worth, **to this port**.
//!
//! `cargo run -p tsr-conformance --example writer_guards --release`
//!
//! ADR-0039 established that upstream's `.types` writer prints
//! `t.AsIntrinsicType().IntrinsicName()` — the literal `"error"` — and falls
//! through to the node builder, which renders any `TypeFlagsAny` type as the
//! `any` keyword, only when a conjunction of **eight guards** holds
//! (`internal/testutil/tsbaseline/type_symbol_baseline.go:380`). One of the
//! eight, `isIntrinsicJsxTag`, is ported (`d6dc9a7`). The other seven are not.
//!
//! `bd tsr-d6o` recorded the *ceilings* by grep — 69,195 `: any` lines in cases
//! carrying an `.errors.txt`, 19,045 in cases without — and recorded, in capital
//! letters, that those are **not deliverables**. Most of an upstream `: any` is a
//! genuine `anyType` that this producer already prints as `any`. The gain is only
//! the subset where **our** checker computes `errorType` *and* the guard would
//! convert it *and* upstream's answer is `any`. This probe measures that subset.
//!
//! # The instrument
//!
//! One arm per guard, attributed in **upstream's written order**, because the
//! condition is a conjunction and the first guard to fire is the one that decides
//! the line. That ordering is what makes `hadErrorBaseline` — which is
//! case-scoped, not positional — absorb everything in a case that has an
//! `.errors.txt`, exactly as upstream does.
//!
//! Every arm splits the lines it claims three ways by **upstream's** answer:
//!
//! | column | meaning |
//! |---|---|
//! | `-> any` | **the deliverable.** We print `error`, upstream prints `any`, the guard converts. |
//! | `-> error` | **must be 0.** A firing guard means upstream took the node builder, which cannot print `error`. Non-zero falsifies the arm. |
//! | `-> other` | no change. Still wrong, differently wrong. |
//!
//! `NO GUARD` is the control bucket and is printed unconditionally, including at
//! zero. It is the only arm where `-> error` *should* be large: those are the
//! lines where our error set and upstream's coincide, which is the one piece of
//! evidence available that we reach `errorType` by upstream's path rather than by
//! falling off a dispatch.
//!
//! # Why the `hadErrorBaseline` arm cannot regress the metric
//!
//! Upstream prints `: error` **zero** times in a case that has an `.errors.txt`
//! (0 violations in 12,155 baselines, `bd tsr-d6o`). So every line this arm
//! claims is already mismatched today. Converting it to `any` can move it from
//! wrong to right or leave it wrong; no currently-right line is reachable. The
//! `-> error` column re-measures that prediction on *aligned* lines rather than
//! by grep, and a non-zero reading there retracts the whole arm.
//!
//! `WRITER_GUARDS_FLATTEN=1` is the mutation: it drops the `hadErrorBaseline`
//! arm from the attribution order, so its lines redistribute across the seven
//! positional arms and `NO GUARD` rather than vanishing. Under it the totals must
//! be unchanged and `NO GUARD` must absorb the bulk — if the positional arms
//! swallow it instead, the arms are not disjoint from the case-scoped guard and
//! the per-position split above it is not readable.

use std::collections::HashMap;

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The eight guards of `type_symbol_baseline.go:380`, in the order the
/// conjunction writes them, plus the control bucket.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Guard {
    HadErrorBaseline,
    BindingElement,
    PropertyAccessOrQualifiedName,
    LabelName,
    GlobalScopeAugmentation,
    MetaProperty,
    ImportStatementName,
    ExportStatementName,
    IntrinsicJsxTag,
    /// No guard fires: upstream keeps the fast path and prints `error` too.
    None,
}

impl Guard {
    const ORDER: [Guard; 10] = [
        Guard::HadErrorBaseline,
        Guard::BindingElement,
        Guard::PropertyAccessOrQualifiedName,
        Guard::LabelName,
        Guard::GlobalScopeAugmentation,
        Guard::MetaProperty,
        Guard::ImportStatementName,
        Guard::ExportStatementName,
        Guard::IntrinsicJsxTag,
        Guard::None,
    ];

    fn label(self) -> &'static str {
        match self {
            Guard::HadErrorBaseline => "hadErrorBaseline (case-scoped)",
            Guard::BindingElement => "binding element parent",
            Guard::PropertyAccessOrQualifiedName => "property access / qualified name parent",
            Guard::LabelName => "label name",
            Guard::GlobalScopeAugmentation => "global scope augmentation",
            Guard::MetaProperty => "meta property",
            Guard::ImportStatementName => "import statement name",
            Guard::ExportStatementName => "export statement name",
            Guard::IntrinsicJsxTag => "intrinsic JSX tag  [PORTED d6dc9a7]",
            Guard::None => "NO GUARD (control bucket)",
        }
    }

    /// Whether this arm is already ported, and so is expected to read near zero
    /// because the producer has already converted its lines away from `error`.
    fn ported(self) -> bool {
        matches!(self, Guard::IntrinsicJsxTag)
    }
}

/// Which guard claims this node, in upstream's conjunction order.
///
/// Upstream's `IsTypeAny(t)` precondition is applied by the caller: only lines
/// where *our* producer prints `error` are offered here, since a line we already
/// print as `any` cannot move whichever way the guards fall.
fn guard_of(nodes: &NodeTable, map: &NodeMap, id: NodeId, has_error_baseline: bool) -> Guard {
    if has_error_baseline && !flatten() {
        return Guard::HadErrorBaseline;
    }
    let Some(parent) = nodes.parent(id) else { return Guard::None };
    match nodes.kind(parent) {
        SyntaxKind::BindingElement => return Guard::BindingElement,
        SyntaxKind::PropertyAccessExpression | SyntaxKind::QualifiedName => {
            return Guard::PropertyAccessOrQualifiedName;
        }
        _ => {}
    }
    if is_label_name(nodes, map, id) {
        return Guard::LabelName;
    }
    if is_global_scope_augmentation(map, parent) {
        return Guard::GlobalScopeAugmentation;
    }
    if nodes.kind(parent) == SyntaxKind::MetaProperty {
        return Guard::MetaProperty;
    }
    if is_import_statement_name(map, parent, id) {
        return Guard::ImportStatementName;
    }
    if is_export_statement_name(map, parent, id) {
        return Guard::ExportStatementName;
    }
    if is_intrinsic_jsx_tag(nodes, map, id) {
        return Guard::IntrinsicJsxTag;
    }
    Guard::None
}

fn flatten() -> bool {
    std::env::var_os("WRITER_GUARDS_FLATTEN").is_some()
}

/// Which *role* inside its parent a claimed node plays.
///
/// `docs/conventions.md`: "a row named after a position is usually not about
/// that position". `!ast.IsPropertyAccessOrQualifiedName(node.Parent)` is one
/// guard over four unrelated nodes — the receiver and the name of `a.b`, and the
/// left and right of `A.B` — and the port already answers two of them by other
/// routes (`type_at_location`'s `bd tsr-tl8` rule for the name, and its
/// qualified-name-left rule). Only the split says which sub-position the arm's
/// lines are actually in, and therefore whether anything is left to build.
///
/// The discriminator this exists to find is the **label-arm signature**: a
/// position whose `-> any` equals its claims with an empty `-> other`, which is
/// what "upstream holds `errorType` here, always" looks like from a baseline and
/// is the one shape that converts without false credit.
fn role_of(nodes: &NodeTable, map: &NodeMap<'_>, id: NodeId) -> &'static str {
    let Some(parent) = nodes.parent(id) else { return "no parent" };
    match map.get(parent) {
        Some(Node::PropertyAccessExpression(n)) => {
            if n.name.and_then(|name| name.node_id()) == Some(id) {
                "property access: the name `b` of `a.b`"
            } else {
                "property access: the receiver `a` of `a.b`"
            }
        }
        Some(Node::QualifiedName(n)) => {
            if n.right.and_then(|right| right.node_id) == Some(id) {
                "qualified name: the right `B` of `A.B`"
            } else {
                "qualified name: the left `A` of `A.B`"
            }
        }
        Some(Node::BindingElement(n)) => {
            if n.name.and_then(|name| name.node_id()) == Some(id) {
                "binding element: the bound name"
            } else if n.property_name.and_then(|name| name.node_id()) == Some(id) {
                "binding element: the property name"
            } else {
                "binding element: elsewhere (initialiser, dotdotdot)"
            }
        }
        Some(Node::ImportSpecifier(n)) => {
            if n.property_name.and_then(|name| name.node_id()) == Some(id) {
                "import: the property name of `{a as b}`"
            } else {
                "import: the local name of `{a}` / `{a as b}`"
            }
        }
        Some(Node::ImportClause(_)) => "import: the default clause name",
        Some(Node::ImportEqualsDeclaration(_)) => "import: the name of `import x = ...`",
        Some(Node::ExportSpecifier(n)) => {
            if n.property_name.and_then(|name| name.node_id()) == Some(id) {
                "export: the property name of `{a as b}`"
            } else {
                "export: the exported name of `{a}` / `{a as b}`"
            }
        }
        Some(Node::ExportAssignment(_)) => "export: the expression of `export = x`",
        _ => "another parent",
    }
}

/// `ast.IsLabelName` (`internal/ast/utilities.go:2263`): the label of a labeled
/// statement, or the target of a `break`/`continue`.
fn is_label_name(nodes: &NodeTable, map: &NodeMap, id: NodeId) -> bool {
    if nodes.kind(id) != SyntaxKind::Identifier {
        return false;
    }
    let Some(parent) = nodes.parent(id) else { return false };
    let label = match map.get(parent) {
        Some(Node::LabeledStatement(n)) => n.label,
        Some(Node::BreakStatement(n)) => n.label,
        Some(Node::ContinueStatement(n)) => n.label,
        _ => return false,
    };
    label.and_then(|label| label.node_id) == Some(id)
}

/// `ast.IsGlobalScopeAugmentation` (`internal/ast/utilities.go:1690`): a module
/// declaration whose keyword is `global`, as in `declare global { … }`.
fn is_global_scope_augmentation(map: &NodeMap, parent: NodeId) -> bool {
    matches!(map.get(parent), Some(Node::ModuleDeclaration(n)) if n.keyword.kind == SyntaxKind::GlobalKeyword)
}

/// `isImportStatementName` (`type_symbol_baseline.go:458`).
fn is_import_statement_name(map: &NodeMap, parent: NodeId, id: NodeId) -> bool {
    match map.get(parent) {
        Some(Node::ImportSpecifier(n)) => {
            n.name.and_then(|name| name.node_id) == Some(id)
                || n.property_name.and_then(|name| name.node_id()) == Some(id)
        }
        Some(Node::ImportClause(n)) => n.name.and_then(|name| name.node_id) == Some(id),
        Some(Node::ImportEqualsDeclaration(n)) => n.name.and_then(|name| name.node_id) == Some(id),
        _ => false,
    }
}

/// `isExportStatementName` (`type_symbol_baseline.go:471`).
fn is_export_statement_name(map: &NodeMap, parent: NodeId, id: NodeId) -> bool {
    match map.get(parent) {
        Some(Node::ExportAssignment(n)) => {
            n.expression.and_then(|expression| expression.node_id()) == Some(id)
        }
        Some(Node::ExportSpecifier(n)) => {
            n.name.and_then(|name| name.node_id()) == Some(id)
                || n.property_name.and_then(|name| name.node_id()) == Some(id)
        }
        _ => false,
    }
}

/// `isIntrinsicJsxTag` (`type_symbol_baseline.go:481`), duplicated here rather
/// than shared with `types_producer`, which keeps its copy private: the probe
/// must be able to size the guard independently of how the producer applies it.
fn is_intrinsic_jsx_tag(nodes: &NodeTable, map: &NodeMap, id: NodeId) -> bool {
    if nodes.kind(id) != SyntaxKind::Identifier {
        return false;
    }
    let Some(parent) = nodes.parent(id) else { return false };
    let tag = match map.get(parent) {
        Some(Node::JsxOpeningElement(n)) => n.tag_name,
        Some(Node::JsxClosingElement(n)) => n.tag_name,
        Some(Node::JsxSelfClosingElement(n)) => n.tag_name,
        _ => return false,
    };
    if tag.and_then(|tag| tag.node_id()) != Some(id) {
        return false;
    }
    let Some(Node::Identifier(name)) = map.get(id) else { return false };
    // `scanner.IsIntrinsicJsxName` (`internal/scanner/utilities.go:98`).
    name.text.starts_with(|first: char| first.is_ascii_lowercase()) || name.text.contains('-')
}

#[derive(Default)]
struct Tally {
    cases: usize,
    cases_with_error_baseline: usize,
    aligned: usize,
    aligned_right: usize,
    aligned_gap: usize,
    /// Aligned lines in cases that have an `.errors.txt`, and of those, how many
    /// upstream prints as `: error`. The second must be zero.
    aligned_in_error_cases: usize,
    upstream_error_in_error_cases: usize,
    /// Per guard: (upstream says `any`, upstream says `error`, upstream says
    /// anything else) over lines where *we* print `error`.
    split: HashMap<Guard, [usize; 3]>,
    /// Per guard, the substitution we would leave behind on an `-> other` line.
    other_subst: HashMap<(Guard, String), usize>,
    /// The same three-way split, one level finer, for the four guards that name
    /// a parent kind rather than a position. See [`role_of`].
    by_role: HashMap<&'static str, [usize; 3]>,
    /// Converting lines per (role, case), so the concentration check can be run
    /// on a sub-position rather than only on the whole conversion. A row that is
    /// half one file is a different item from one spread over 2,000 cases, and
    /// that is decided per row, not per table.
    role_cases: HashMap<(&'static str, String), usize>,
    /// Converted lines per case, for the concentration check.
    per_case: HashMap<String, usize>,
    /// For each case with at least one converted line: (converted, still wrong
    /// or gapped afterwards). The level-4 statistic — predicts *cases*, not lines.
    residue: Vec<(usize, usize)>,
}

impl Tally {
    fn merge(&mut self, other: Self) {
        self.cases += other.cases;
        self.cases_with_error_baseline += other.cases_with_error_baseline;
        self.aligned += other.aligned;
        self.aligned_right += other.aligned_right;
        self.aligned_gap += other.aligned_gap;
        self.aligned_in_error_cases += other.aligned_in_error_cases;
        self.upstream_error_in_error_cases += other.upstream_error_in_error_cases;
        for (key, value) in other.split {
            let entry = self.split.entry(key).or_insert([0; 3]);
            for (slot, add) in entry.iter_mut().zip(value) {
                *slot += add;
            }
        }
        for (key, value) in other.other_subst {
            *self.other_subst.entry(key).or_default() += value;
        }
        for (key, value) in other.by_role {
            let entry = self.by_role.entry(key).or_insert([0; 3]);
            for (slot, add) in entry.iter_mut().zip(value) {
                *slot += add;
            }
        }
        for (key, value) in other.role_cases {
            *self.role_cases.entry(key).or_default() += value;
        }
        for (key, value) in other.per_case {
            *self.per_case.entry(key).or_default() += value;
        }
        self.residue.extend(other.residue);
    }
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus unavailable: git submodule update --init --recursive");
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

            // Upstream's `hasErrorBaseline` is `len(result.Diagnostics) > 0`
            // (`internal/testrunner/compiler_runner.go:501`), and it writes the
            // `.errors.txt` exactly when that holds. The baseline's presence is
            // therefore the faithful observable, and it is *case*-scoped: one
            // program, every file in it.
            let has_error_baseline =
                case.has_varied_errors() || matches!(case.expected_errors(), Ok(Some(_)));
            if has_error_baseline {
                tally.cases_with_error_baseline = 1;
            }

            // **Through the harness, not around it.** The first version of this
            // probe parsed, bound and checked each unit itself, so it loaded no
            // `lib.*.d.ts` and measured a different compiler than the gradient it
            // quoted. `assertions_for_case_with_ids` is the same program the
            // suite judges, and hands back the `NodeId` behind each line so the
            // guard arms can be asked positionally. See the convention added in
            // `dc61c84`.
            let arena = tsr_core::Arena::new();
            let (program, rendered, ids) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
            let nodes = program.nodes();
            let map = program.node_map();

            let mut converted = 0;
            let mut residue = 0;

            for (section, expected_file) in expected.iter().enumerate() {
                let (Some(ours), Some(section_ids)) = (rendered.get(section), ids.get(section))
                else {
                    continue;
                };
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = ours.get(position) else { continue };
                    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    tally.aligned += 1;
                    let correct = want_type == got.type_string;
                    if correct {
                        tally.aligned_right += 1;
                    } else if got.type_string == "error" {
                        tally.aligned_gap += 1;
                    }
                    if has_error_baseline {
                        tally.aligned_in_error_cases += 1;
                        if want_type == "error" {
                            tally.upstream_error_in_error_cases += 1;
                        }
                    }

                    // Upstream's `IsTypeAny(t)` precondition, in our terms: only
                    // a line we print as `error` can move. A genuine `any` we
                    // already print as `any`, and the guards do not change it.
                    if got.type_string != "error" {
                        if !correct {
                            residue += 1;
                        }
                        continue;
                    }
                    let Some(&id) = section_ids.get(position) else { continue };
                    let guard = guard_of(nodes, map, id, has_error_baseline);
                    let slot = match want_type {
                        "any" => 0,
                        "error" => 1,
                        _ => 2,
                    };
                    tally.split.entry(guard).or_insert([0; 3])[slot] += 1;
                    if slot == 2 {
                        *tally.other_subst.entry((guard, want_type.to_string())).or_default() += 1;
                    }
                    // The four arms named after a *parent kind* rather than a
                    // position, split one level finer. Not mine — the sub-position
                    // splitter and its two tallies were already in this file when
                    // I rewired the loop, and this is its call site restored.
                    if matches!(
                        guard,
                        Guard::BindingElement
                            | Guard::PropertyAccessOrQualifiedName
                            | Guard::ImportStatementName
                            | Guard::ExportStatementName
                    ) {
                        let role = role_of(nodes, map, id);
                        tally.by_role.entry(role).or_insert([0; 3])[slot] += 1;
                        if slot == 0 {
                            *tally
                                .role_cases
                                .entry((role, case.stem().to_string()))
                                .or_default() += 1;
                        }
                    }
                    if guard != Guard::None && slot == 0 {
                        converted += 1;
                    } else if !correct {
                        residue += 1;
                    }
                }
            }

            if converted > 0 {
                *tally.per_case.entry(case.stem().to_string()).or_default() += converted;
                tally.residue.push((converted, residue));
            }
            tally
        })
        .reduce(Tally::default, |mut a, b| {
            a.merge(b);
            a
        });

    report(&total);
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 }
}

#[allow(clippy::too_many_lines)]
fn report(total: &Tally) {
    if flatten() {
        println!("*** WRITER_GUARDS_FLATTEN=1 — the hadErrorBaseline arm is disabled ***\n");
    }
    println!("cases judged                {:>9}", total.cases);
    println!(
        "  with an errors baseline   {:>9}   {:.1}%",
        total.cases_with_error_baseline,
        pct(total.cases_with_error_baseline, total.cases)
    );
    println!("aligned lines               {:>9}", total.aligned);
    println!(
        "  of which RIGHT            {:>9}   {:.4}%  <- the gradient",
        total.aligned_right,
        pct(total.aligned_right, total.aligned)
    );
    println!(
        "  of which gap (we print `error`)  {:>4}   wrong {}",
        total.aligned_gap,
        total.aligned - total.aligned_right - total.aligned_gap
    );
    println!();

    println!("--- the file-scoped prediction, re-measured on ALIGNED lines ---");
    println!(
        "aligned lines in cases WITH an errors baseline   {:>8}",
        total.aligned_in_error_cases
    );
    println!(
        "  of those, upstream prints `: error`           {:>8}   <- MUST BE 0",
        total.upstream_error_in_error_cases
    );
    println!();

    println!("--- what each guard is worth: lines where WE print `error` ---");
    println!(
        "{:<42} {:>8} {:>9} {:>9} {:>9}",
        "guard (upstream's conjunction order)", "claims", "-> any", "-> error", "-> other"
    );
    let mut deliverable = 0;
    let mut contradictions = 0;
    for guard in Guard::ORDER {
        let [any, error, other] = total.split.get(&guard).copied().unwrap_or([0; 3]);
        let claims = any + error + other;
        if guard != Guard::None {
            deliverable += any;
            contradictions += error;
        }
        let note = if guard == Guard::None {
            "  (upstream prints `error` too)"
        } else if guard.ported() {
            "  (already ported)"
        } else {
            ""
        };
        println!("{:<42} {claims:>8} {any:>9} {error:>9} {other:>9}{note}", guard.label());
    }
    println!();
    println!(
        "DELIVERABLE (unported guards excluded below) {:>8} lines   {:.4}% of aligned",
        deliverable,
        pct(deliverable, total.aligned)
    );
    let ported: usize = Guard::ORDER
        .iter()
        .filter(|g| g.ported())
        .map(|g| total.split.get(g).copied().unwrap_or([0; 3])[0])
        .sum();
    println!(
        "  minus the already-ported JSX arm           {:>8} lines   {:.4}% of aligned",
        deliverable - ported,
        pct(deliverable - ported, total.aligned)
    );
    println!(
        "  gradient after, if every one converts      {:.4}%",
        pct(total.aligned_right + deliverable - ported, total.aligned)
    );
    println!(
        "CONTRADICTIONS (a firing guard whose line upstream prints `error`) {contradictions}   <- MUST BE 0"
    );
    println!();

    println!("--- the four parent-kind arms, split by the position inside the parent ---");
    let mut roles: Vec<_> = total.by_role.iter().map(|(k, v)| (*k, *v)).collect();
    roles.sort_by(|a, b| (b.1[0] + b.1[1] + b.1[2]).cmp(&(a.1[0] + a.1[1] + a.1[2])));
    println!(
        "{:<48} {:>8} {:>9} {:>9} {:>9}",
        "position", "claims", "-> any", "-> error", "-> other"
    );
    for (role, [any, error, other]) in &roles {
        println!("{role:<48} {:>8} {any:>9} {error:>9} {other:>9}", any + error + other);
    }
    println!();

    // The concentration check, **per sub-position**, for the ones that carry the
    // label-arm signature — an empty `-> other` column. Those are the only rows
    // a port can convert without the false-credit discount, so they are the only
    // ones worth ranking by case.
    println!("--- concentration of the clean sub-positions (empty `-> other`) ---");
    for (role, [any, _, other]) in &roles {
        if *other != 0 || *any == 0 {
            continue;
        }
        let mut rows: Vec<_> = total
            .role_cases
            .iter()
            .filter(|((r, _), _)| r == role)
            .map(|((_, case), n)| (*n, case.clone()))
            .collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let sum: usize = rows.iter().map(|(n, _)| n).sum();
        let top10: usize = rows.iter().take(10).map(|(n, _)| n).sum();
        println!(
            "  {role}: {sum} lines over {} cases, top 10 hold {:.1}%",
            rows.len(),
            pct(top10, sum)
        );
        for (n, case) in rows.iter().take(10) {
            println!("    {n:>5}  {case}");
        }
    }
    println!();

    println!("--- concentration: converted lines by case ---");
    let mut rows: Vec<_> = total.per_case.iter().map(|(k, v)| (*v, k.clone())).collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let converted_total: usize = rows.iter().map(|(n, _)| n).sum();
    println!("cases with >=1 converted line   {:>6}", rows.len());
    println!("converted lines                 {converted_total:>6}");
    let top1 = rows.first().map_or(0, |(n, _)| *n);
    let top10: usize = rows.iter().take(10).map(|(n, _)| n).sum();
    println!("  top case                      {:>6}   {:.1}%", top1, pct(top1, converted_total));
    println!("  top 10 cases                  {:>6}   {:.1}%", top10, pct(top10, converted_total));
    for (n, stem) in rows.iter().take(15) {
        println!("    {n:>6}  {stem}");
    }
    println!();

    println!("--- level 4: what else still fails in a case this touches ---");
    let mut clean = 0;
    let mut residue_total = 0;
    for (_, residue) in &total.residue {
        residue_total += residue;
        if *residue == 0 {
            clean += 1;
        }
    }
    println!(
        "cases left with NO other defect {:>6}   of {} touched   {:.1}%",
        clean,
        total.residue.len(),
        pct(clean, total.residue.len())
    );
    println!("remaining defects in touched cases {residue_total:>6}");
    println!();

    println!("--- the `-> other` lines: what upstream says instead ---");
    for guard in Guard::ORDER {
        if guard == Guard::None {
            continue;
        }
        let mut rows: Vec<_> = total
            .other_subst
            .iter()
            .filter(|((g, _), _)| *g == guard)
            .map(|((_, theirs), n)| (*n, theirs.clone()))
            .collect();
        if rows.is_empty() {
            continue;
        }
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        println!("  {}", guard.label());
        for (n, theirs) in rows.iter().take(8) {
            println!("    {n:>6}  upstream {theirs}");
        }
    }
}
