//! What the cross-file module seam is worth: the two ALIAS rows sized
//! separately, everything downstream of them enumerated, and the answer each
//! form would give **once the seam is removed** — `bd tsr-mmd`.
//!
//! `cargo run -p tsr-conformance --example module_blocked --release`
//!
//! # The question
//!
//! Two agents are plumbing module resolution into `Checker::new`. The number
//! being quoted for it is **4,156 lines**: `rank_board`'s
//! `declaration name … SymbolFlags(ALIAS) / no value declaration` (2,535) plus
//! its sibling `reference … SymbolFlags(ALIAS) / no value declaration` (1,621).
//! That number is a **population ceiling over two rows whose `KIND` label was
//! never established** — both landed in `cause()`'s default arm, which
//! `docs/architecture/checker-notes-rank.md`'s correction header shows means
//! "none of three evidence patterns matched" and not "the prerequisite is met".
//! A declaration name is a leaf and a reference identifier is a leaf, so the
//! span test cannot fire on either; the label is decided by construction and
//! carries no information.
//!
//! So this probe does not inherit a `KIND`. It establishes one from the thing
//! that actually decides it — **the alias declaration's form** — and then asks
//! the only question that separates a population from a conversion:
//! `docs/conventions.md`, *"for a dependency-gated form, probe past the
//! blocker, not at it"*. For every cross-file alias line it resolves the module
//! specifier against the program the harness already built, looks the target
//! export up in that file's module symbol, asks `get_type_of_symbol` for it and
//! **prints the answer against upstream's**. If the mocked answer is still
//! `error`, the seam was real and not sufficient, and that is learned here for
//! the cost of a bucket rather than after the build.
//!
//! # Units
//!
//! Every number is an **assertion line** in a `.types` baseline unless the row
//! says `cases`. No count here is a node count.
//!
//! # The four outputs, in the order they change a decision
//!
//! 1. **The two rows sized separately**, each with its own form table, its own
//!    same-file/cross-file split, and its own concentration (cases, top-1,
//!    top-10). The board's two rows have opposite case profiles — `finishes`
//!    117 against 0 — which is on its own a reason not to add them.
//! 2. **What the seam answers once removed**, per form.
//! 3. **What else the seam unblocks.** The two rows are where an alias
//!    *declaration name* and an alias *reference* gap. Every property access
//!    through an import, every call to one, and every initialiser fed by one
//!    lands in a different row. This walks each gap line down to its blocking
//!    leaves and reports only the lines whose blockers are **all** cross-file
//!    aliases. The lines that merely *touch* a cross-file alias are printed
//!    beside them, because that sum answers a different question — the one that
//!    over-counted by 2.7× in `docs/conventions.md`.
//! 4. **Level 4**, per population, with the **`0` bucket printed as its own
//!    row**.
//!
//! # Pre-registered decision rules — written before the first run
//!
//! `docs/conventions.md`: *"pre-register on the most direct bucket your
//! instrument produces, not on a proxy"*. Each rule below names the bucket it
//! reads, and each bucket is printed whether or not the rule fires.
//!
//! - **R1 (cases, declaration-name row).** Read the **`0` bucket** of the
//!   level-4 histogram taken over the cases the row's **cross-file** half
//!   touches, using the **mock-converts** population and not the ceiling. The
//!   seam is worth building *for the case gate* if that bucket is **≥ 5.0%** of
//!   those cases. 5.0% is not arbitrary: `export { q }` — the only arm of this
//!   family ever built — flipped 10 of the 193 cases its half touched, 5.2%,
//!   and it is the honest prior for a sibling arm.
//! - **R2 (cases, reference row).** The board reports `finishes 0` for this
//!   row, so the prediction is that its **`0` bucket reads 0.0%**. If it
//!   exceeds **2.0%**, the board's zero was an artefact of asking the question
//!   one row at a time, and this page must say so.
//! - **R3 (lines, both rows).** Read `WOULD CONVERT` as a share of the
//!   cross-file lines this probe could mock. If it is **< 50%**, the seam is
//!   **necessary and not sufficient**, and the only quotable number for the
//!   work is the mocked conversion — not 4,156, and not the cross-file
//!   population either. The prior is `export { q }`'s measured **47.5%**, which
//!   sits just under the threshold on purpose: a rule that the only measured
//!   precedent passes comfortably is not a test.
//! - **R4 (what must not move).** The **same-file** halves of both rows are not
//!   on this seam at all. Their line counts must be reported separately and
//!   must never appear inside a total quoted for module resolution.
//!   `export { q }` is same-file and was **built this cycle** (`c60b086`,
//!   +94 lines / +10 cases, 47.5%); its residue is reported and excluded.
//!
//! # Controls, printed unconditionally
//!
//! `docs/conventions.md`, *"prefer a control pinned by construction over one
//! pinned by arithmetic"*. Three of the five below are pinned by the subject
//! and are the ones that can see a semantic inversion:
//!
//! - **S1 — seed lines have nothing gapped inside them.** An alias declaration
//!   name and an alias reference are both **leaves**: `x` in
//!   `import { x } from "./m"` spans `x` and contains no other assertion line.
//!   So the count of seed lines with a gapped line strictly inside their span is
//!   **0, and that was true before this file was written**. It reads non-zero if
//!   the span test's polarity is inverted — the exact defect
//!   `docs/conventions.md` records an agent introducing by copying this
//!   expression out of `rank_board`.
//! - **S2 — a same-file alias form has no module specifier.** `export { q }`
//!   and `import a = b` cannot carry one; the count of same-file seeds from
//!   which a specifier was extracted is **0** by the grammar.
//! - **S3 — a cross-file alias form has one.** The mirror, and it fails in the
//!   opposite direction, so the two together pin the form classifier rather
//!   than one arm of it.
//! - **A1 — right + gap + wrong = aligned**, and the gradient is re-derived and
//!   printed so this probe's denominator can be reconciled against the suite's
//!   before anything here is quoted (`docs/conventions.md`: *"a probe's
//!   denominator must be the gradient's by construction"* — it is, this routes
//!   through `types_producer::assertions_for_case_with_ids`).
//! - **A2 — the blocker walk partitions the gap.** The four buckets seam-only,
//!   mixed, no-alias and unresolved sum to every gap line. `UNRESOLVED` is a
//!   real arm with a positive test, not a default: it holds the lines whose
//!   named dependency this probe could not follow to a line, and it is the
//!   honest size of what the walk does not account for.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The two rows the assignment names, matched in full.
const ROW_DECL: &str =
    "declaration name, symbol has no type: SymbolFlags(ALIAS) / no value declaration";
const ROW_REF: &str = "reference, symbol has no type: SymbolFlags(ALIAS) / no value declaration";

/// Whether an alias form needs a module graph to answer.
///
/// This — not `cause()`'s default arm — is what decides the kind of a line in
/// either row. Taken from `examples/symbol_dispatch_split.rs:329`, whose
/// classification is the one `docs/architecture/checker-notes-symbols.md` §4
/// quotes; reproduced rather than shared because an example cannot import
/// another example, and the two are cross-checked in this probe's output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reach {
    /// Answerable inside the file that declares it — kind 1, not this seam.
    SameFile,
    /// Needs the specifier resolved to another file — this seam.
    CrossFile,
    /// Neither: `export as namespace N`'s target is the file's own module
    /// symbol, so it needs no module graph and is not a local form either.
    Neither,
}

/// What a gap line is waiting on, once followed to its blocking leaves.
///
/// A set rather than a label, because *"all of this line's blockers are the
/// seam"* and *"one of them is"* are different claims and only the first sizes
/// the work.
// Five bools rather than a bitflags type: each is a distinct claim about the
// line and they are read individually, which is exactly what `seam_only`
// depends on.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Blockers {
    cross: bool,
    same: bool,
    neither: bool,
    /// A gap that is not an alias at all — its own rule is missing.
    other: bool,
    /// The named dependency could not be followed to an assertion line. Not
    /// evidence of either answer; counted as its own control.
    unresolved: bool,
}

impl Blockers {
    fn union(self, other: Self) -> Self {
        Self {
            cross: self.cross || other.cross,
            same: self.same || other.same,
            neither: self.neither || other.neither,
            other: self.other || other.other,
            unresolved: self.unresolved || other.unresolved,
        }
    }

    /// Every blocker of this line is the cross-file seam.
    fn seam_only(self) -> bool {
        self.cross && !self.same && !self.neither && !self.other && !self.unresolved
    }

    fn any(self) -> bool {
        self.cross || self.same || self.neither || self.other || self.unresolved
    }
}

/// What the seam would answer for one cross-file line, with the blocker mocked
/// out: the specifier resolved by hand against the program the harness built,
/// and the target export's type asked for directly.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Mock {
    /// The specifier named no file in the program — an ambient module, a
    /// `node_modules` package, or a path this probe's resolver does not model.
    /// The seam does not convert these either.
    NoSuchFile,
    /// The file is in the program and has no export under that name.
    NoSuchExport,
    /// The target is a namespace-shaped form (`import * as ns`,
    /// `import a = require(...)`, `export * as ns`) whose answer is a module
    /// object type, which this probe does not build. Named rather than folded.
    NotMocked,
    /// The target resolved and `get_type_of_symbol` still answered `error` —
    /// **the seam is necessary and not sufficient for this line**.
    StillError,
    /// The target resolved to a type, and it is the wrong one.
    WouldBeWrong,
    /// The target resolved to a type and it is upstream's, exactly.
    WouldConvert,
}

impl Mock {
    fn label(self) -> &'static str {
        match self {
            Self::NoSuchFile => "specifier names no file in the program",
            Self::NoSuchExport => "the file has no export of that name",
            Self::NotMocked => "namespace-shaped target, not mocked here",
            Self::StillError => "target found, still `error` — NEXT BLOCKER",
            Self::WouldBeWrong => "target found, typed, WRONG type",
            Self::WouldConvert => "WOULD CONVERT — exactly upstream's type",
        }
    }
}

/// One seed line: a gap line whose own reason is an alias with no value
/// declaration.
struct Seed {
    /// `ROW_DECL`, `ROW_REF`, or another position with the same flags.
    row: String,
    form: String,
    reach: Reach,
    mock: Option<Mock>,
    /// Whether a module specifier was extracted — controls S2 and S3.
    has_specifier: bool,
    /// Whether anything gapped strictly inside this line's span — control S1.
    gapped_inside: bool,
    /// The specifier text, kept for the unresolved-specifier dump: a control
    /// that only prints a number cannot be acted on when it is non-zero.
    specifier: Option<String>,
}

/// One case's contribution.
#[derive(Default)]
struct CaseReport {
    name: String,
    expected: usize,
    matched: usize,
    aligned: usize,
    gap: usize,
    wrong: usize,
    /// Seed lines, in no order.
    seeds: Vec<Seed>,
    /// Gap lines whose blockers are all the cross-file seam, by row.
    seam_only: BTreeMap<String, usize>,
    /// Gap lines that *touch* a cross-file alias, by row. A superset, and the
    /// sum that answers a different question.
    touches: BTreeMap<String, usize>,
    /// Control A2's four buckets over every gap line.
    walk_seam_only: usize,
    walk_mixed: usize,
    walk_no_alias: usize,
    walk_unresolved: usize,
    /// Lines this case would convert under each population, for level 4.
    decl_cross: usize,
    decl_cross_converts: usize,
    ref_cross: usize,
    ref_cross_converts: usize,
    /// The seam-only downstream lines, which is the whole-seam population.
    seam_all: usize,
    seam_all_converts: usize,
    /// Every seam-only line by the **worst** mock outcome underneath it. This
    /// is the line-level answer to *"what would the seam deliver"*, and it is
    /// not the row sizes: a call expression waiting on a namespace-shaped
    /// import is seam-only and unconvertible until the module object type
    /// exists.
    seam_worst: BTreeMap<Mock, usize>,
    /// Seeds whose specifier names a `declare module "x"` in the same program.
    ambient_specifier: usize,
    /// Cross-file seeds in this case whose target this probe could not mock —
    /// a namespace-shaped form, or a specifier naming no file in the program.
    ///
    /// A case holding one of these has a **depressed** mock-converts count, so
    /// the level-4 table repeats itself over the cases where this is zero. That
    /// third reading is neither the ceiling nor a lower bound: it is the
    /// measurement, on the sub-population where the mock is complete.
    unmockable_seeds: usize,
}

/// `rank_board::row_key` (`examples/rank_board.rs:145`), verbatim, so a row name
/// here is the same string the board prints.
fn row_key(reason: &str) -> String {
    for cut in ["has no such property: ", "unresolved: "] {
        if let Some(at) = reason.find(cut) {
            return reason[..at + cut.len() - 2].to_string();
        }
    }
    reason.to_string()
}

/// Whether a reason string is an alias-with-no-value-declaration gap.
///
/// Wider than the two rows the assignment names, deliberately: the same seam
/// gaps `SymbolFlags(ALIAS | TYPE)` and the `the name of a …` positions, and
/// those are part of *"what else does this unblock"*. The two named rows are
/// still reported alone.
fn is_alias_reason(reason: &str) -> bool {
    reason.contains("SymbolFlags(ALIAS") && reason.contains("/ no value declaration")
}

/// The two properties the classifier rests on, checked before anything is
/// measured and on every run. `rank_board` puts these in `main` rather than in
/// `#[cfg(test)]` for a reason this probe shares: Cargo does not run tests
/// inside an example unless the manifest declares `test = true`, and the
/// manifest is shared with three other agents this cycle.
fn check_classifier() {
    assert!(is_alias_reason(ROW_DECL) && is_alias_reason(ROW_REF), "both named rows are seeds");
    assert!(
        is_alias_reason(
            "declaration name, symbol has no type: SymbolFlags(ALIAS | TYPE) / no value declaration"
        ),
        "a merged alias is the same seam"
    );
    // The near miss that must NOT be a seed: an export marker has no value
    // declaration either and is a different work item, closed in `af7f12a`.
    assert!(
        !is_alias_reason(
            "reference, symbol has no type: SymbolFlags(EXPORT_VALUE) / no value declaration"
        ),
        "an export marker is not an alias"
    );
    assert_eq!(row_key(ROW_DECL), ROW_DECL, "row_key is the identity on the named rows");
    // A blocker set with one non-alias member is not the seam, however many
    // alias members it has. This is the 2.7× rule, as an assertion.
    let mixed = Blockers { cross: true, other: true, ..Blockers::default() };
    assert!(!mixed.seam_only(), "one non-alias blocker takes a line out of the seam population");
    assert!(Blockers { cross: true, ..Blockers::default() }.seam_only());
}

fn main() {
    check_classifier();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let reports: Vec<CaseReport> = cases.par_iter().filter_map(measure).collect();
    report(&reports);
}

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<CaseReport> {
    // The suite's own skips, skip for skip.
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
    let node_map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::new(bound, nodes, node_map);
    // A second checker for the mock, so asking for a target's type cannot
    // perturb the memo the reason strings are read out of — the same separation
    // `examples/symbol_dispatch_split.rs:209` makes, and for the same reason.
    let mut mock_checker = tsr_checker::Checker::new(bound, nodes, node_map);
    let error = mock_checker.intrinsics().error;

    let mut report = CaseReport { name: case.name.clone(), ..CaseReport::default() };

    // Every `declare module "x"` in the program, by its quoted name. A
    // specifier naming one of these is **not** a file-graph question: upstream
    // resolves it against the ambient module table, which is a different work
    // item from the seam and would otherwise be invisible inside this probe's
    // `specifier names no file` bucket.
    let ambient: BTreeSet<String> = program
        .source_files()
        .iter()
        .flat_map(|file| file.source_file().statements.iter())
        .filter_map(|statement| match statement {
            tsr_ast::Statement::ModuleDeclaration(module) => match module.name {
                Some(tsr_ast::ModuleName::StringLiteral(name)) => Some(name.text.to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect();

    for (index, expected_file) in expected.iter().enumerate() {
        let our_file = ours.get(index);
        let our_ids = ids.get(index);

        // `rank_board`'s span test. `nothing_below[i]` is true when NOTHING
        // gapped inside line `i` — the name states the polarity, because
        // reproducing this expression with the polarity flipped is a defect
        // `docs/conventions.md` records. Control S1 is what catches it.
        let nothing_below: Vec<bool> = match (our_file, our_ids) {
            (Some(file), Some(line_ids)) if file.len() == line_ids.len() => (0..file.len())
                .map(|i| {
                    if file[i].type_string != "error" {
                        return true;
                    }
                    let outer = nodes.span(line_ids[i]);
                    !(i + 1..file.len())
                        .take_while(|&j| {
                            let inner = nodes.span(line_ids[j]);
                            inner.start >= outer.start && inner.end <= outer.end
                        })
                        .any(|j| file[j].type_string == "error")
                })
                .collect(),
            _ => Vec::new(),
        };

        // Every rendered line's node, so a named dependency — a receiver, an
        // initialiser, an annotation — can be followed to the line that holds
        // it instead of being given up on.
        let mut position_of: HashMap<NodeId, usize> = HashMap::new();
        if let Some(line_ids) = our_ids {
            for (position, id) in line_ids.iter().enumerate() {
                position_of.entry(*id).or_insert(position);
            }
        }

        // Pass one: the reason and the wanted type for every aligned gap line.
        let mut reasons: HashMap<usize, String> = HashMap::new();
        let mut wants: HashMap<usize, String> = HashMap::new();
        for (position, want) in expected_file.assertions.iter().enumerate() {
            report.expected += 1;
            let Some(got) = our_file.and_then(|file| file.get(position)) else { continue };
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            report.aligned += 1;
            if want_type == got.type_string {
                report.matched += 1;
                continue;
            }
            if got.type_string != "error" {
                report.wrong += 1;
                continue;
            }
            report.gap += 1;
            let id = our_ids.expect("ids beside a rendered file")[position];
            reasons.insert(
                position,
                types_producer::gap_reason(&mut checker, bound, nodes, node_map, id),
            );
            wants.insert(position, want_type.to_string());
        }

        let line_ids = match our_ids {
            Some(line_ids) => line_ids.as_slice(),
            None => continue,
        };
        let file_lines = match our_file {
            Some(file) => file.as_slice(),
            None => continue,
        };

        // Pass two: classify the seeds, and mock past the blocker.
        let mut seed_of: HashMap<usize, Reach> = HashMap::new();
        let mut mock_of: HashMap<usize, Mock> = HashMap::new();
        for (&position, reason) in &reasons {
            if !is_alias_reason(reason) {
                continue;
            }
            let id = line_ids[position];
            let Some(symbol) = seed_symbol(bound, nodes, node_map, id, reason) else { continue };
            let Some(&declaration) = bound.symbols().get(symbol).declarations.first() else {
                continue;
            };
            let (form, reach) = alias_form(node_map, nodes, declaration);
            let specifier = module_specifier(node_map, nodes, declaration);
            let mock = match reach {
                Reach::CrossFile => Some(mock_target(
                    &program,
                    &mut mock_checker,
                    bound,
                    nodes,
                    node_map,
                    declaration,
                    specifier.as_deref(),
                    wants.get(&position).map_or("", String::as_str),
                    error,
                )),
                _ => None,
            };
            if mock == Some(Mock::NoSuchFile)
                && specifier.as_deref().is_some_and(|text| ambient.contains(text))
            {
                report.ambient_specifier += 1;
            }
            seed_of.insert(position, reach);
            if let Some(mock) = mock {
                mock_of.insert(position, mock);
            }
            report.seeds.push(Seed {
                row: row_key(reason),
                form,
                reach,
                mock,
                has_specifier: specifier.is_some(),
                specifier: specifier.clone(),
                gapped_inside: !nothing_below.get(position).copied().unwrap_or(true),
            });
            let converts = mock == Some(Mock::WouldConvert);
            if reach == Reach::CrossFile
                && !matches!(mock, Some(Mock::StillError | Mock::WouldBeWrong | Mock::WouldConvert))
            {
                report.unmockable_seeds += 1;
            }
            if reason == ROW_DECL && reach == Reach::CrossFile {
                report.decl_cross += 1;
                report.decl_cross_converts += usize::from(converts);
            }
            if reason == ROW_REF && reach == Reach::CrossFile {
                report.ref_cross += 1;
                report.ref_cross_converts += usize::from(converts);
            }
        }

        // Pass three: walk every gap line down to its blocking leaves.
        let mut memo: HashMap<usize, (Blockers, Option<Mock>)> = HashMap::new();
        for (&position, reason) in &reasons {
            let mut stack = Vec::new();
            let (blockers, worst) = blockers_of(
                position,
                &reasons,
                &seed_of,
                &mock_of,
                &position_of,
                file_lines,
                line_ids,
                nodes,
                node_map,
                bound,
                &mut memo,
                &mut stack,
            );
            let row = row_key(reason);
            if blockers.cross {
                *report.touches.entry(row.clone()).or_default() += 1;
            }
            if blockers.seam_only() {
                *report.seam_only.entry(row).or_default() += 1;
                report.walk_seam_only += 1;
                report.seam_all += 1;
                // A line converts only if **every** cross-file alias under it
                // would — and this still assumes the intermediate rule is
                // ported, which is why the level-4 table prints the ceiling
                // beside it rather than only this.
                report.seam_all_converts += usize::from(worst == Some(Mock::WouldConvert));
                if let Some(worst) = worst {
                    *report.seam_worst.entry(worst).or_default() += 1;
                }
            } else if blockers.cross {
                report.walk_mixed += 1;
            } else if blockers.unresolved {
                report.walk_unresolved += 1;
            } else {
                report.walk_no_alias += 1;
            }
        }
    }
    Some(report)
}

/// The symbol behind a seed line, re-derived by the same test `gap_reason` used
/// to name it — the declaration-name branch takes the parent's symbol
/// (`types_producer.rs:1093`) and the identifier branch resolves the name in
/// value space (`:1114`). Using the wrong one silently reclassifies lines.
fn seed_symbol(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    node_map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
    reason: &str,
) -> Option<tsr_binder::SymbolId> {
    if reason.starts_with("declaration name,") {
        return nodes.parent(id).and_then(|parent| bound.symbol_of(parent));
    }
    let Some(Node::Identifier(name)) = node_map.get(id) else { return None };
    bound.resolve_name(nodes, node_map, id, name.text, SymbolFlags::VALUE)
}

/// Which alias form this is, and whether it can be answered without a module
/// graph. Reproduces `examples/symbol_dispatch_split.rs:329`.
fn alias_form(
    node_map: &tsr_ast::NodeMap<'_>,
    nodes: &tsr_ast::NodeTable,
    declaration: NodeId,
) -> (String, Reach) {
    match node_map.get(declaration) {
        Some(Node::ImportEqualsDeclaration(node)) => match node.module_reference {
            Some(tsr_ast::ModuleReference::Identifier(_)) => {
                ("import a = b            (same file)".to_string(), Reach::SameFile)
            }
            Some(tsr_ast::ModuleReference::QualifiedName(_)) => {
                ("import a = b.c          (same file)".to_string(), Reach::SameFile)
            }
            Some(tsr_ast::ModuleReference::ExternalModuleReference(_)) => {
                ("import a = require(...) (cross file)".to_string(), Reach::CrossFile)
            }
            None => ("import a = <missing>".to_string(), Reach::Neither),
        },
        Some(Node::ImportClause(_)) => {
            ("import d from           (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::ImportSpecifier(_)) => {
            ("import { x } from       (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::NamespaceImport(_)) => {
            ("import * as ns from     (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::NamespaceExport(_)) => {
            ("export * as ns from     (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::ExportSpecifier(_)) => {
            if export_declaration_of(node_map, nodes, declaration)
                .is_some_and(|export| export.module_specifier.is_some())
            {
                ("export { q } from       (cross file)".to_string(), Reach::CrossFile)
            } else {
                ("export { q }            (SAME FILE, built c60b086)".to_string(), Reach::SameFile)
            }
        }
        Some(Node::ExportAssignment(node)) => match node.expression {
            Some(tsr_ast::Expression::Identifier(_)) if node.is_export_equals => {
                ("export = x              (SAME FILE)".to_string(), Reach::SameFile)
            }
            Some(tsr_ast::Expression::Identifier(_)) => {
                ("export default x        (SAME FILE)".to_string(), Reach::SameFile)
            }
            _ => ("export default <expr>   (SAME FILE)".to_string(), Reach::SameFile),
        },
        Some(Node::NamespaceExportDeclaration(_)) => {
            ("export as namespace N   (neither half)".to_string(), Reach::Neither)
        }
        _ => (format!("UNCLASSIFIED FORM: {:?}", nodes.kind(declaration)), Reach::Neither),
    }
}

/// The `ExportDeclaration` two levels above an `ExportSpecifier`.
fn export_declaration_of<'a>(
    node_map: &tsr_ast::NodeMap<'a>,
    nodes: &tsr_ast::NodeTable,
    specifier: NodeId,
) -> Option<&'a tsr_ast::ExportDeclaration<'a>> {
    let clause = nodes.parent(specifier)?;
    let export = nodes.parent(clause)?;
    match node_map.get(export)? {
        Node::ExportDeclaration(declaration) => Some(declaration),
        _ => None,
    }
}

/// The module specifier an alias declaration was written with, as text.
///
/// `None` for every same-file form, which is control S2: those grammars have no
/// place to put one.
fn module_specifier(
    node_map: &tsr_ast::NodeMap<'_>,
    nodes: &tsr_ast::NodeTable,
    declaration: NodeId,
) -> Option<String> {
    let literal = |expression: Option<tsr_ast::Expression<'_>>| match expression {
        Some(tsr_ast::Expression::StringLiteral(string)) => Some(string.text.to_string()),
        _ => None,
    };
    match node_map.get(declaration)? {
        Node::ImportEqualsDeclaration(node) => match node.module_reference {
            Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) => {
                literal(reference.expression)
            }
            _ => None,
        },
        Node::ImportClause(_) | Node::ImportSpecifier(_) | Node::NamespaceImport(_) => {
            // The `ImportDeclaration` is one, two or three levels up depending
            // on the form; walking to it is exact where counting levels is not.
            let mut current = nodes.parent(declaration);
            while let Some(id) = current {
                if let Some(Node::ImportDeclaration(import)) = node_map.get(id) {
                    return literal(import.module_specifier);
                }
                current = nodes.parent(id);
            }
            None
        }
        Node::NamespaceExport(_) | Node::ExportSpecifier(_) => {
            let mut current = nodes.parent(declaration);
            while let Some(id) = current {
                if let Some(Node::ExportDeclaration(export)) = node_map.get(id) {
                    return literal(export.module_specifier);
                }
                current = nodes.parent(id);
            }
            None
        }
        _ => None,
    }
}

/// **Probe past the blocker.** Resolve the specifier against the program the
/// harness already built, find the target export, and print what its type would
/// be against upstream's.
///
/// This is the whole difference between a population and a conversion. It is a
/// *mock*: it does what `Checker::new` will be able to do once it holds the
/// module graph, by hand and outside the checker. If the answer is still
/// `error`, the seam is real and not sufficient.
#[allow(clippy::too_many_arguments)]
fn mock_target(
    program: &tsr_compiler::Program<'_>,
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    node_map: &tsr_ast::NodeMap<'_>,
    declaration: NodeId,
    specifier: Option<&str>,
    want: &str,
    error: tsr_checker::TypeId,
) -> Mock {
    // The name to look up in the target module, and `None` for the
    // namespace-shaped forms whose answer is a module object type.
    let name = match node_map.get(declaration) {
        Some(Node::ImportSpecifier(node)) => match node.property_name {
            Some(property) => Some(module_export_name(property)),
            None => node.name.map(|name| name.text.to_string()),
        },
        Some(Node::ImportClause(_)) => Some("default".to_string()),
        Some(Node::ExportSpecifier(node)) => {
            node.property_name.or(node.name).map(module_export_name)
        }
        _ => None,
    };
    let Some(name) = name else { return Mock::NotMocked };
    let Some(specifier) = specifier else { return Mock::NoSuchFile };
    let Some(file) = resolve_specifier(program, nodes, declaration, specifier) else {
        return Mock::NoSuchFile;
    };
    let Some(root) = file.source_file().node_id else { return Mock::NoSuchFile };
    let Some(module) = bound.symbol_of(root) else { return Mock::NoSuchExport };
    let Some(&target) = bound.symbols().get(module).exports.get(name.as_str()) else {
        return Mock::NoSuchExport;
    };
    let answer = checker.get_type_of_symbol(target);
    if answer == error {
        return Mock::StillError;
    }
    if checker.type_to_string(answer) == want { Mock::WouldConvert } else { Mock::WouldBeWrong }
}

fn module_export_name(name: tsr_ast::ModuleExportName<'_>) -> String {
    match name {
        tsr_ast::ModuleExportName::Identifier(identifier) => identifier.text.to_string(),
        tsr_ast::ModuleExportName::StringLiteral(string) => string.text.to_string(),
    }
}

/// The program file a relative specifier names.
///
/// A deliberately small resolver: the corpus's multi-file cases are
/// `// @filename:` units imported by relative path, so joining against the
/// containing file's directory and trying the extension order covers them. A
/// specifier it cannot place is reported as `NoSuchFile` rather than guessed
/// at, and that bucket is printed — so the coverage of this resolver is visible
/// in the output instead of being a claim in this comment.
fn resolve_specifier<'a>(
    program: &'a tsr_compiler::Program<'a>,
    nodes: &tsr_ast::NodeTable,
    declaration: NodeId,
    specifier: &str,
) -> Option<&'a tsr_compiler::ProgramFile<'a>> {
    let containing = program.source_files().iter().find(|file| file.contains(declaration))?;
    let directory = containing.file_name().rsplit_once('/').map_or("", |(head, _)| head);
    let joined = if directory.is_empty() {
        specifier.to_string()
    } else {
        format!("{directory}/{specifier}")
    };
    let extensions =
        ["", ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", "/index.ts", "/index.d.ts"];
    // `import "./m.js"` names `m.ts`: the specifier carries the *output*
    // extension. Without this the probe reports its own resolver's limit as an
    // unresolvable module, which is the shape of error `docs/conventions.md`
    // calls measuring a different compiler.
    let trimmed: Vec<String> = [".js", ".mjs", ".cjs", ".jsx"]
        .iter()
        .filter_map(|extension| joined.strip_suffix(extension).map(ToString::to_string))
        .collect();
    let mut bases: Vec<&str> = vec![joined.as_str(), specifier];
    bases.extend(trimmed.iter().map(String::as_str));
    for base in bases {
        for extension in extensions {
            if let Some(file) = program.source_file(&format!("{base}{extension}")) {
                // A file cannot import itself into existence: a specifier that
                // resolves back to the containing file is a resolver artefact,
                // not a cross-file edge.
                if file.file_name() != containing.file_name() {
                    return Some(file);
                }
            }
        }
    }
    let _ = nodes;
    None
}

/// Every blocker of a gap line, followed to the leaves.
///
/// Three ways down, in the order the evidence is strongest:
///
/// 1. the line **is** a seed — its own reason names an alias;
/// 2. the reason names a dependency the span test cannot see — a receiver, an
///    initialiser, an annotation — and that dependency is followed to its line;
/// 3. otherwise the maximal gapped lines strictly inside the span.
///
/// A line with none of the three is its own missing rule: `other`. That is an
/// **explicit terminal arm**, not a default — `docs/conventions.md`'s rule that
/// the semantically loaded label must never be the `else` branch. The `else`
/// branch here is `unresolved`, which claims nothing.
#[allow(clippy::too_many_arguments)]
fn blockers_of(
    position: usize,
    reasons: &HashMap<usize, String>,
    seed_of: &HashMap<usize, Reach>,
    mock_of: &HashMap<usize, Mock>,
    position_of: &HashMap<NodeId, usize>,
    lines: &[types_producer::Assertion],
    line_ids: &[NodeId],
    nodes: &tsr_ast::NodeTable,
    node_map: &tsr_ast::NodeMap<'_>,
    bound: &tsr_binder::BindResult<'_>,
    memo: &mut HashMap<usize, (Blockers, Option<Mock>)>,
    stack: &mut Vec<usize>,
) -> (Blockers, Option<Mock>) {
    if let Some(&known) = memo.get(&position) {
        return known;
    }
    if stack.contains(&position) {
        // A cycle is not evidence of anything.
        return (Blockers { unresolved: true, ..Blockers::default() }, None);
    }
    stack.push(position);
    let answer = (|| {
        if let Some(&reach) = seed_of.get(&position) {
            let mock = mock_of.get(&position).copied();
            return match reach {
                Reach::CrossFile => (Blockers { cross: true, ..Blockers::default() }, mock),
                Reach::SameFile => (Blockers { same: true, ..Blockers::default() }, None),
                Reach::Neither => (Blockers { neither: true, ..Blockers::default() }, None),
            };
        }
        let Some(reason) = reasons.get(&position) else {
            return (Blockers { unresolved: true, ..Blockers::default() }, None);
        };
        // A named sibling dependency: the receiver of a property access, or the
        // initialiser / annotation of the declaration a name belongs to. These
        // are exactly the cases `rank_board`'s span test cannot see, and the
        // reason `DEPENDENT-UNKNOWN` exists on the board.
        if let Some(dependency) =
            named_dependency(position, reason, line_ids, nodes, node_map, bound)
        {
            return match position_of.get(&dependency) {
                Some(&at) if at != position => blockers_of(
                    at,
                    reasons,
                    seed_of,
                    mock_of,
                    position_of,
                    lines,
                    line_ids,
                    nodes,
                    node_map,
                    bound,
                    memo,
                    stack,
                ),
                _ => (Blockers { unresolved: true, ..Blockers::default() }, None),
            };
        }
        // The maximal gapped lines strictly inside this one's span.
        let mut answer = Blockers::default();
        // The **worst** mock outcome underneath, by `Mock`'s declaration order:
        // one namespace-shaped seed under a call expression is enough to stop
        // the whole line converting, and taking the minimum says so.
        let mut worst: Option<Mock> = None;
        let outer = nodes.span(line_ids[position]);
        let mut index = position + 1;
        let mut covered = outer.start;
        while index < lines.len() {
            let inner = nodes.span(line_ids[index]);
            if !(inner.start >= outer.start && inner.end <= outer.end) {
                break;
            }
            if lines[index].type_string == "error" && inner.start >= covered {
                covered = inner.end;
                let (below, below_worst) = blockers_of(
                    index,
                    reasons,
                    seed_of,
                    mock_of,
                    position_of,
                    lines,
                    line_ids,
                    nodes,
                    node_map,
                    bound,
                    memo,
                    stack,
                );
                answer = answer.union(below);
                worst = match (worst, below_worst) {
                    (Some(left), Some(right)) => Some(left.min(right)),
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (None, None) => None,
                };
            }
            index += 1;
        }
        if answer.any() {
            (answer, worst)
        } else {
            (Blockers { other: true, ..Blockers::default() }, None)
        }
    })();
    stack.pop();
    memo.insert(position, answer);
    answer
}

/// The node a reason string names as the thing that gapped, when the span test
/// cannot see it.
fn named_dependency(
    position: usize,
    reason: &str,
    line_ids: &[NodeId],
    nodes: &tsr_ast::NodeTable,
    node_map: &tsr_ast::NodeMap<'_>,
    bound: &tsr_binder::BindResult<'_>,
) -> Option<NodeId> {
    let id = line_ids[position];
    if reason.contains("the receiver is a gap") {
        // For `member name, …` the node is the property name and the access is
        // its parent; for `property access, …` the node is the access itself.
        let access = if reason.starts_with("member name,") { nodes.parent(id)? } else { id };
        return match node_map.get(access)? {
            Node::PropertyAccessExpression(node) => node.expression.and_then(|e| e.node_id()),
            _ => None,
        };
    }
    if reason.contains("/ initialiser ") || reason.contains("/ annotation ") {
        let symbol = nodes.parent(id).and_then(|parent| bound.symbol_of(parent))?;
        let declaration = bound.symbols().get(symbol).value_declaration?;
        let node = node_map.get(declaration)?;
        return node.type_id().or_else(|| node.initializer_id());
    }
    None
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

fn delta(left: usize, right: usize) -> i128 {
    i128::try_from(left).unwrap_or_default() - i128::try_from(right).unwrap_or_default()
}

/// Cases, top-1 share, top-10 share, over a population keyed by case.
fn concentration(by_case: &BTreeMap<String, usize>) -> (usize, f64, f64) {
    let total: usize = by_case.values().sum();
    let mut counts: Vec<usize> = by_case.values().copied().collect();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    let top1 = counts.first().copied().unwrap_or_default();
    let top10: usize = counts.iter().take(10).sum();
    (counts.len(), pct(top1, total), pct(top10, total))
}

/// The level-4 histogram, with the `0` bucket as its own row and first.
fn level_four(label: &str, residuals: &[usize]) {
    println!("  {label} — {} cases", residuals.len());
    let zero = residuals.iter().filter(|&&r| r == 0).count();
    println!(
        "      remaining       0: {zero:>5} cases  {:>5.1}%   <- THE BUCKET THE RULE READS",
        pct(zero, residuals.len())
    );
    for (low, high, name) in [
        (1usize, 6usize, "1-5"),
        (6, 11, "6-10"),
        (11, 26, "11-25"),
        (26, 101, "26-100"),
        (101, usize::MAX, ">100"),
    ] {
        let n = residuals.iter().filter(|&&r| r >= low && r < high).count();
        println!("      remaining {name:>7}: {n:>5} cases  {:>5.1}%", pct(n, residuals.len()));
    }
}

#[allow(clippy::too_many_lines)]
fn report(reports: &[CaseReport]) {
    let expected: usize = reports.iter().map(|r| r.expected).sum();
    let aligned: usize = reports.iter().map(|r| r.aligned).sum();
    let matched: usize = reports.iter().map(|r| r.matched).sum();
    let gap: usize = reports.iter().map(|r| r.gap).sum();
    let wrong: usize = reports.iter().map(|r| r.wrong).sum();

    println!("cases judged:             {}", reports.len());
    println!("assertion lines upstream: {expected}");
    println!("  aligned:                {aligned} ({:.2}%)", pct(aligned, expected));
    println!(
        "  exactly right:          {matched} ({:.2}% of upstream lines)  <- reconcile against the \
         gradient before quoting anything below",
        pct(matched, expected)
    );
    println!("  gap  (we said `error`): {gap}");
    println!("  wrong (ported, defect): {wrong}");
    println!(
        "\nCONTROL A1 right+gap+wrong-aligned = {} (must be 0)",
        delta(matched + gap + wrong, aligned)
    );

    // ---- the two rows, separately ----
    for (row, title) in [(ROW_DECL, "ROW A — declaration name"), (ROW_REF, "ROW B — reference")]
    {
        let seeds: Vec<(&str, &Seed)> = reports
            .iter()
            .flat_map(|report| report.seeds.iter().map(move |seed| (report.name.as_str(), seed)))
            .filter(|(_, seed)| seed.row == row)
            .collect();
        println!("\n=== {title}: {} LINES ===", seeds.len());
        println!("  {row}");
        let mut by_case: BTreeMap<String, usize> = BTreeMap::new();
        let mut forms: BTreeMap<&str, (usize, Reach)> = BTreeMap::new();
        let mut by_reach: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
        let mut mocks: BTreeMap<Mock, usize> = BTreeMap::new();
        for (case, seed) in &seeds {
            *by_case.entry((*case).to_string()).or_default() += 1;
            let entry = forms.entry(seed.form.as_str()).or_insert((0, seed.reach));
            entry.0 += 1;
            let reach = match seed.reach {
                Reach::SameFile => "SAME FILE",
                Reach::CrossFile => "CROSS FILE",
                Reach::Neither => "neither",
            };
            *by_reach.entry(reach).or_default().entry((*case).to_string()).or_default() += 1;
            if let Some(mock) = seed.mock {
                *mocks.entry(mock).or_default() += 1;
            }
        }
        let (cases, top1, top10) = concentration(&by_case);
        println!(
            "  lines {:>6}   cases {cases:>5}   top-1 {top1:>5.1}%   top-10 {top10:>5.1}%",
            seeds.len()
        );
        println!("\n  forms (lines):");
        let mut form_rows: Vec<_> = forms.iter().collect();
        form_rows.sort_unstable_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
        for (form, (count, reach)) in form_rows {
            println!("    {count:>6}  {form}  [{reach:?}]");
        }
        println!("\n  KIND, established from the form and not inherited from `cause()`:");
        for reach in ["CROSS FILE", "SAME FILE", "neither"] {
            let Some(by_case) = by_reach.get(reach) else { continue };
            let total: usize = by_case.values().sum();
            let (cases, top1, top10) = concentration(by_case);
            let kind = match reach {
                "CROSS FILE" => "kind 2 — blocked on the module seam",
                "SAME FILE" => "kind 1 — NOT on this seam (R4)",
                _ => "neither half",
            };
            println!(
                "    {reach:<11} {total:>6} lines  {cases:>5} cases  top-1 {top1:>5.1}%  top-10 \
                 {top10:>5.1}%  {kind}"
            );
        }
        println!("\n  PAST THE BLOCKER — what the cross-file half answers with the seam mocked:");
        let mocked: usize = mocks.values().sum();
        for (mock, count) in &mocks {
            println!("    {count:>6}  {:>5.1}%  {}", pct(*count, mocked), mock.label());
        }
        let answerable: usize = mocks
            .iter()
            .filter(|(mock, _)| {
                matches!(mock, Mock::StillError | Mock::WouldBeWrong | Mock::WouldConvert)
            })
            .map(|(_, count)| count)
            .sum();
        let converts = mocks.get(&Mock::WouldConvert).copied().unwrap_or_default();
        println!(
            "    R3: WOULD CONVERT / lines this probe could mock = {converts}/{answerable} = \
             {:.1}%",
            pct(converts, answerable)
        );
    }

    // ---- controls over the seeds ----
    let all_seeds: Vec<&Seed> = reports.iter().flat_map(|report| report.seeds.iter()).collect();
    let inside = all_seeds.iter().filter(|seed| seed.gapped_inside).count();
    let same_with =
        all_seeds.iter().filter(|seed| seed.reach == Reach::SameFile && seed.has_specifier).count();
    let cross_without = all_seeds
        .iter()
        .filter(|seed| seed.reach == Reach::CrossFile && !seed.has_specifier)
        .count();
    println!(
        "\nCONTROL S1 seed lines with a gapped line inside their span = {inside} (must be 0 — a seed is a leaf)"
    );
    println!(
        "CONTROL S2 same-file seeds carrying a module specifier    = {same_with} (must be 0 — the grammar has no place for one)"
    );
    println!(
        "CONTROL S3 cross-file seeds with no module specifier      = {cross_without} (must be 0)"
    );
    let unclassified =
        all_seeds.iter().filter(|seed| seed.form.starts_with("UNCLASSIFIED FORM")).count();
    println!("CONTROL     seeds whose declaration form is unclassified  = {unclassified}");
    // A control that prints only a number cannot be acted on when it reads
    // non-zero, so each one prints what fell through it.
    for (label, matches) in [
        ("S1", &(|seed: &Seed| seed.gapped_inside) as &dyn Fn(&Seed) -> bool),
        ("S3", &|seed: &Seed| seed.reach == Reach::CrossFile && !seed.has_specifier),
        ("FORM", &|seed: &Seed| seed.form.starts_with("UNCLASSIFIED FORM")),
    ] {
        let mut forms: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for report in reports {
            for seed in report.seeds.iter().filter(|seed| matches(seed)) {
                *forms.entry((report.name.as_str(), seed.form.as_str())).or_default() += 1;
            }
        }
        for ((case, form), count) in forms.iter().take(12) {
            println!("    {label} fell through: {count:>4}  {form}  {case}");
        }
    }
    // The resolver's own coverage, printed rather than claimed: which
    // specifiers named no file in the program.
    let mut unresolved: BTreeMap<&str, usize> = BTreeMap::new();
    for report in reports {
        for seed in &report.seeds {
            if seed.mock == Some(Mock::NoSuchFile) {
                unresolved
                    .entry(seed.specifier.as_deref().unwrap_or("<none extracted>"))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
        }
    }
    let ambient: usize = reports.iter().map(|report| report.ambient_specifier).sum();
    println!(
        "\nof the lines whose specifier named no file, {ambient} name a `declare module \"x\"` \n\
         in the same program — an ambient-module table, not a file graph, and a separate item"
    );
    let mut unresolved: Vec<_> =
        unresolved.into_iter().map(|(text, count)| (count, text)).collect();
    unresolved.sort_unstable_by(|a, b| b.cmp(a));
    println!(
        "\ncommonest specifiers this probe's resolver could not place ({} distinct):",
        unresolved.len()
    );
    for (count, text) in unresolved.iter().take(12) {
        println!("    {count:>5}  {text}");
    }

    // ---- the seam, whole: every row it blocks ----
    println!("\n=== WHAT ELSE THE SEAM UNBLOCKS ===");
    println!(
        "Two columns, and they answer different questions. `seam-only` is every gap line all of\n\
         whose blocking leaves are cross-file aliases — the population a module graph could\n\
         convert. `touches` is every line with a cross-file alias somewhere underneath, which is\n\
         the sum that over-counted by 2.7x in docs/conventions.md and is printed to be refused."
    );
    let mut seam_rows: BTreeMap<String, (usize, BTreeSet<String>)> = BTreeMap::new();
    let mut touch_rows: BTreeMap<String, usize> = BTreeMap::new();
    for report in reports {
        for (row, count) in &report.seam_only {
            let entry = seam_rows.entry(row.clone()).or_default();
            entry.0 += count;
            entry.1.insert(report.name.clone());
        }
        for (row, count) in &report.touches {
            *touch_rows.entry(row.clone()).or_default() += count;
        }
    }
    let seam_total: usize = seam_rows.values().map(|(count, _)| count).sum();
    let touch_total: usize = touch_rows.values().sum();
    println!(
        "\n{:<62} {:>8} {:>8} {:>7}",
        "row (assertion LINES, not nodes)", "seam-only", "touches", "cases"
    );
    let mut ranked: Vec<_> = seam_rows.iter().collect();
    ranked.sort_unstable_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    for (row, (count, cases)) in ranked.iter().take(25) {
        let touches = touch_rows.get(*row).copied().unwrap_or_default();
        println!("{:<62} {count:>8} {touches:>8} {:>7}", truncate(row, 60), cases.len());
    }
    println!(
        "\n  TOTAL seam-only {seam_total} lines ({:.2}% of the {gap} gap lines)",
        pct(seam_total, gap)
    );
    println!(
        "  TOTAL touches   {touch_total} lines — NOT a work item; {:.2}x the seam-only total",
        {
            #[allow(clippy::cast_precision_loss)]
            {
                touch_total as f64 / seam_total.max(1) as f64
            }
        }
    );

    println!("\n  every seam-only line by the WORST mock outcome underneath it:");
    let mut worst: BTreeMap<Mock, usize> = BTreeMap::new();
    for report in reports {
        for (mock, count) in &report.seam_worst {
            *worst.entry(*mock).or_default() += count;
        }
    }
    let worst_total: usize = worst.values().sum();
    for (mock, count) in &worst {
        println!("    {count:>8}  {:>6.2}%  {}", pct(*count, worst_total), mock.label());
    }
    println!(
        "    CONTROL worst-outcome total - seam-only total = {} (must be 0)",
        delta(worst_total, seam_total)
    );

    let walk_seam: usize = reports.iter().map(|r| r.walk_seam_only).sum();
    let walk_mixed: usize = reports.iter().map(|r| r.walk_mixed).sum();
    let walk_none: usize = reports.iter().map(|r| r.walk_no_alias).sum();
    let walk_unresolved: usize = reports.iter().map(|r| r.walk_unresolved).sum();
    println!("\n  the blocker walk over every gap line:");
    println!(
        "    {walk_seam:>8}  {:>6.2}%  all blockers are the cross-file seam",
        pct(walk_seam, gap)
    );
    println!(
        "    {walk_mixed:>8}  {:>6.2}%  a cross-file alias AND something else",
        pct(walk_mixed, gap)
    );
    println!("    {walk_none:>8}  {:>6.2}%  no cross-file alias underneath", pct(walk_none, gap));
    println!(
        "    {walk_unresolved:>8}  {:>6.2}%  UNRESOLVED — the named dependency reached no line",
        pct(walk_unresolved, gap)
    );
    println!(
        "  CONTROL A2 walk buckets - gap total = {} (must be 0)",
        delta(walk_seam + walk_mixed + walk_none + walk_unresolved, gap)
    );

    // ---- level 4 ----
    println!("\n=== LEVEL 4 — what else still fails in a case this touches ===");
    println!(
        "Residual = upstream lines - exactly right, per case, minus what the population would\n\
         convert in that case. The `0` bucket is the cases the work would FINISH, and it is the\n\
         bucket rules R1 and R2 are written against."
    );
    for (label, take, converted, mockable_only) in [
        ("R1 ceiling  — ROW A cross-file population", 0usize, false, false),
        ("R1 measured — ROW A cross-file mock-converts", 0, true, false),
        ("R1 mockable — ROW A, cases where every seed could be mocked", 0, true, true),
        ("R2 ceiling  — ROW B cross-file population", 1, false, false),
        ("R2 measured — ROW B cross-file mock-converts", 1, true, false),
        ("R2 mockable — ROW B, cases where every seed could be mocked", 1, true, true),
        ("the whole seam — every seam-only line, ceiling", 2, false, false),
        ("the whole seam — mock-converts", 2, true, false),
        ("the whole seam — cases where every seed could be mocked", 2, true, true),
    ] {
        let residuals: Vec<usize> = reports
            .iter()
            .filter_map(|report| {
                if mockable_only && report.unmockable_seeds > 0 {
                    return None;
                }
                let (population, converts) = match take {
                    0 => (report.decl_cross, report.decl_cross_converts),
                    1 => (report.ref_cross, report.ref_cross_converts),
                    _ => (report.seam_all, report.seam_all_converts),
                };
                if population == 0 {
                    return None;
                }
                let residual = report.expected - report.matched;
                let closed = if converted { converts } else { population };
                Some(residual.saturating_sub(closed))
            })
            .collect();
        level_four(label, &residuals);
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.len() <= width { text.to_string() } else { format!("{}…", &text[..width - 1]) }
}
