//! Why a name does not resolve, and whether this port could spell the answer.
//!
//! # The two rows this measures
//!
//! `docs/architecture/checker-notes-recvgap.md` §4 lists
//! `reference, the name does not resolve` at **3,619 lines** as the largest
//! terminal blocker behind the receiver-is-a-gap rows, and
//! `checker-notes-rank.md` §3 lists the same phrase as board row 7 at **3,621
//! lines** in its own right. Those are two different populations — one is the
//! *root* of a chain, the other is the *line itself* — and this probe measures
//! both, keyed the same way, so the cascade multiplier is read off rather than
//! assumed. `docs/conventions.md`: *"a row counts the lines that name a defect,
//! not the lines downstream of them."*
//!
//! It also measures the `no value declaration` family (`checker-notes-rank.md`
//! §1, 5,743 lines), keyed by symbol flags, because that is the other half of
//! the same assignment.
//!
//! # The question, and where the rule is registered
//!
//! The question is **not** "how many lines say the name does not resolve". It is
//! *"how many of them would this port answer correctly if the name resolved"*.
//! Two things stop that being the row size:
//!
//! 1. **Upstream may not resolve the name either.** An unresolved name is
//!    `errorType` upstream, and upstream's node builder prints `errorType` as
//!    `any` (which is why 2,467 of these lines have `any` on the baseline's
//!    right-hand side). For those lines the checker already agrees with
//!    upstream; the *renderer* does not, deliberately, because this port prints
//!    `error` to keep the gap/wrong split. Nothing in `symbols.rs`,
//!    `resolution.rs` or the binder can move them.
//! 2. **The answer may be unspellable.** `docs/conventions.md`, *"before 'how
//!    many lines does this block', ask 'can this port spell the answer?'"*.
//!
//! So the **direct bucket is `RESOLVABLE-AND-UPSTREAM-HAS-A-REAL-TYPE`**: lines
//! where the name is declared somewhere this program already holds *and* the
//! baseline's right-hand side is neither `any` nor `error`. The rule is
//! registered on that bucket and on no quantity derived from it:
//!
//! > **BUILD if the largest single mechanism in that bucket exceeds 800 lines
//! > (direct + cascade) over more than 40 cases with a top-1 case share below
//! > 40%. Otherwise REFUSE.**
//!
//! 800 is ~2.5× the largest arm landed in this area (`export { q }`, 198 lines
//! same-file / +94 measured), so it is a threshold this row has to clear rather
//! than one it clears by existing. The case and concentration legs are there
//! because `docs/conventions.md` records three rows that evaporated on exactly
//! that check.
//!
//! # The second rule, for the `no value declaration` half
//!
//! `Checker::export_symbol_of` (`crates/tsr-checker/src/symbols.rs:162`) resolves
//! an export marker by walking to the marker's **source file** and reading that
//! file's module symbol's `exports`. For `export enum E` inside
//! `namespace N { … }` the export symbol is on **`N`'s** symbol, not the file's,
//! so the lookup misses and `get_type_of_export_value` answers `errorType`.
//! Upstream does not have this problem because it follows a direct link,
//! `symbol.ExportSymbol` (`getExportSymbolOfValueSymbolIfExported`,
//! `checker.go:14383`), which `tsr_binder::Symbol` does not carry.
//!
//! Registered **before** the split below was measured:
//!
//! > **BUILD the container fix if the `EXPORT_VALUE / no value declaration`
//! > lines whose marker declaration sits inside a `ModuleDeclaration` exceed 500
//! > lines (direct + cascade) over more than 40 cases with a top-1 case share
//! > below 50%, and no top-10 baseline right-hand side for them names a file
//! > path. Otherwise REFUSE.**
//!
//! The top-1 leg is the live one: `conformance/parserRealSource10` alone holds
//! 494 of the row's 1,346 lines (36.7%), and `docs/conventions.md` records that
//! family as one hand-written compiler source checked in as a test.
//!
//! # Controls
//!
//! - **C1 (construction).** `gap_reason` emits `the name does not resolve` from
//!   exactly one arm, the `Expression::Identifier` arm
//!   (`types_producer.rs:1131`). So a direct-row line whose node is not an
//!   `Identifier` is **0**, and the mirror (the row total) is printed beside it.
//! - **C2 (construction).** The descent to a chain's root leaves only a
//!   `PropertyAccessExpression`, so a terminal reason still carrying
//!   `the receiver is a gap` is **0**. Copied from `receiver_gap.rs`.
//! - **C3 (construction).** The two rows are disjoint: `gap_reason` returns from
//!   one arm, so no line is in both. **0**.
//! - **C4 (construction, with mirror).** Every line in the `@lib dropped` bucket
//!   names a symbol that is **not** in the loaded program's globals — that is
//!   what the bucket asserts. **0** violations; the mirror is the bucket size.
//! - **C5 (measurement, not construction).** Lines that resolve under
//!   `SymbolFlags::all()` where the `VALUE` lookup failed. Non-zero would mean
//!   the row is partly a *meaning* bug rather than a declaration bug.
//! - **A1 (arithmetic).** Buckets sum to the row.
//!
//! `Bucket::Unclassified` is a real residue: every other arm carries a positive
//! test and the name-existence arms are asked in a fixed order, so a name that
//! matches none of them lands there. `docs/conventions.md`: *"never let the
//! semantically loaded label be the default arm."*
//!
//! Run: `cargo run --release -p tsr-conformance --example nameres`

use std::collections::{BTreeMap, HashSet};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Verbatim from `types_producer::gap_reason`'s identifier arm.
const UNRESOLVED: &str = "the name does not resolve";
/// Verbatim from `types_producer::access_reason`.
const RECEIVER_GAP: &str = "the receiver is a gap";
/// Verbatim from `types_producer::gap_reason`'s `describe`.
const NO_VALUE_DECL: &str = "no value declaration";

/// Which population a line belongs to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Row {
    /// The line *is* the unresolved name. Board row 7.
    Direct,
    /// The line is a member access whose chain roots at an unresolved name.
    /// `checker-notes-recvgap.md` §4.
    Cascade,
}

impl Row {
    const fn label(self) -> &'static str {
        match self {
            Self::Direct => "DIRECT  (the line is the name)",
            Self::Cascade => "CASCADE (a member access rooted at it)",
        }
    }
}

/// Why the name did not resolve. First match wins; the order is by causal
/// strength, and every arm but [`Bucket::Unclassified`] has a positive test.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Bucket {
    /// The case sets `@lib`, which the harness's `apply_test_directives` does
    /// not read, so the program was built with the default lib set.
    LibDropped,
    /// `globalThis`. Unported; not a lookup failure.
    GlobalThis,
    /// `arguments`. Upstream synthesises it in `initializeChecker`; the binder
    /// deliberately does not (`binder.rs`, `declare_synthesised_globals`).
    Arguments,
    /// A `CommonJS` ambient this port does not synthesise.
    CommonJsAmbient,
    /// A symbol of this name is declared inside a `namespace`/`module` body
    /// somewhere in this program. `resolve_name` has no namespace-exports arm.
    InNamespaceBody,
    /// A symbol of this name exists in this program, outside a namespace body.
    ElsewhereInProgram,
    /// No symbol of this name anywhere in the program. Upstream cannot resolve
    /// it either; the case is an error test.
    NowhereInProgram,
    /// The identifier carries no text. Parser error recovery.
    EmptyName,
    /// None of the above matched. Must be reachable, and is.
    Unclassified,
}

impl Bucket {
    const fn label(self) -> &'static str {
        match self {
            Self::LibDropped => "@lib directive dropped by the harness",
            Self::GlobalThis => "globalThis (unported global)",
            Self::Arguments => "arguments (unsynthesised global)",
            Self::CommonJsAmbient => "CommonJS ambient (require/module/exports/...)",
            Self::InNamespaceBody => "declared inside a namespace body (no exports arm)",
            Self::ElsewhereInProgram => "declared elsewhere in this program",
            Self::NowhereInProgram => "not declared anywhere (upstream fails too)",
            Self::EmptyName => "empty identifier text (parser recovery)",
            Self::Unclassified => "UNCLASSIFIED",
        }
    }
}

/// What upstream prints for the *root identifier's own* baseline line.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Upstream {
    /// `any` or `error` — upstream's own `errorType`. We already agree.
    AlsoAnError,
    /// A real type. These are the only lines a resolution fix could convert.
    RealType,
    /// The baseline line for the root could not be located.
    Unknown,
}

impl Upstream {
    const fn label(self) -> &'static str {
        match self {
            Self::AlsoAnError => "upstream also errors (`any`/`error`)",
            Self::RealType => "upstream has a REAL type",
            Self::Unknown => "root's baseline line not located",
        }
    }
}

/// Where an export marker's declaration sits, which decides whether
/// `export_symbol_of`'s file-level lookup could ever have found it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Container {
    /// An ancestor `ModuleDeclaration` stands between the declaration and the
    /// file, so the export symbol is on the namespace, not the file.
    InNamespace,
    /// No `ModuleDeclaration` ancestor: the file-level lookup was addressed at
    /// the right table and still missed, so the cause is something else.
    AtFileTop,
    /// The marker has no declaration to walk from.
    NoDeclaration,
}

impl Container {
    const fn label(self) -> &'static str {
        match self {
            Self::InNamespace => "inside a namespace (export_symbol_of reads the wrong table)",
            Self::AtFileTop => "at file top level (right table, still missed)",
            Self::NoDeclaration => "marker has no declaration",
        }
    }
}

#[derive(Default)]
struct Tally {
    lines: usize,
    per_case: BTreeMap<String, usize>,
}

impl Tally {
    fn add(&mut self, case: &str, n: usize) {
        self.lines += n;
        *self.per_case.entry(case.to_string()).or_default() += n;
    }

    fn merge(&mut self, other: &Self) {
        self.lines += other.lines;
        for (case, n) in &other.per_case {
            *self.per_case.entry(case.clone()).or_default() += n;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.per_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let total = self.lines.max(1) as f64;
        let top1 = counts.first().copied().unwrap_or(0) as f64 / total * 100.0;
        let top10 = counts.iter().take(10).sum::<usize>() as f64 / total * 100.0;
        (counts.len(), top1, top10)
    }

    fn top_cases(&self, n: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> =
            self.per_case.iter().map(|(c, v)| (c.clone(), *v)).collect();
        entries.sort_by_key(|(c, v)| (std::cmp::Reverse(*v), c.clone()));
        entries.truncate(n);
        entries
    }
}

#[derive(Default)]
struct Report {
    right: usize,
    gap: usize,
    wrong: usize,
    /// The whole partition: (row, bucket, upstream) -> lines.
    cells: BTreeMap<(Row, Bucket, Upstream), Tally>,
    /// Identifier text histogram, direct row only.
    names: BTreeMap<String, usize>,
    /// The baseline right-hand side of the root's own line, per bucket. The
    /// spellability check.
    spelling: BTreeMap<(Bucket, String), usize>,
    /// The `no value declaration` family, keyed by the flags half of the reason.
    no_value_decl: BTreeMap<String, Tally>,
    /// `no value declaration` lines by what upstream prints.
    no_value_decl_rhs: BTreeMap<String, usize>,
    /// The `EXPORT_VALUE` marker split: (row, container) -> lines.
    export_value: BTreeMap<(Row, Container), Tally>,
    /// Per container bucket, what the baseline prints for the marker's own line.
    export_value_rhs: BTreeMap<(Container, String), usize>,
    /// The two preconditions `export_symbol_of` needs, counted per line.
    /// `file_has_no_symbol`: the marker's source file has no module symbol at
    /// all, so the lookup cannot even start. `lookup_would_hit`: the file's own
    /// exports table *does* hold the name, so `export_symbol_of` succeeded and
    /// the gap is downstream of it.
    file_has_no_symbol: usize,
    lookup_would_hit: usize,
    c1_direct_not_identifier: usize,
    c1_direct_total: usize,
    c2_terminal_is_access: usize,
    c3_in_both_rows: usize,
    c4_lib_bucket_name_in_globals: usize,
    c4_lib_bucket_total: usize,
    c5_resolves_under_wider_meaning: usize,
}

impl Report {
    fn merge(&mut self, o: &Self) {
        self.right += o.right;
        self.gap += o.gap;
        self.wrong += o.wrong;
        for (k, v) in &o.cells {
            self.cells.entry(*k).or_default().merge(v);
        }
        for (k, v) in &o.names {
            *self.names.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.spelling {
            *self.spelling.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.no_value_decl {
            self.no_value_decl.entry(k.clone()).or_default().merge(v);
        }
        for (k, v) in &o.no_value_decl_rhs {
            *self.no_value_decl_rhs.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.export_value {
            self.export_value.entry(*k).or_default().merge(v);
        }
        for (k, v) in &o.export_value_rhs {
            *self.export_value_rhs.entry(k.clone()).or_default() += v;
        }
        self.file_has_no_symbol += o.file_has_no_symbol;
        self.lookup_would_hit += o.lookup_would_hit;
        self.c1_direct_not_identifier += o.c1_direct_not_identifier;
        self.c1_direct_total += o.c1_direct_total;
        self.c2_terminal_is_access += o.c2_terminal_is_access;
        self.c3_in_both_rows += o.c3_in_both_rows;
        self.c4_lib_bucket_name_in_globals += o.c4_lib_bucket_name_in_globals;
        self.c4_lib_bucket_total += o.c4_lib_bucket_total;
        self.c5_resolves_under_wider_meaning += o.c5_resolves_under_wider_meaning;
    }
}

/// Asserted on every run, before anything is measured.
fn check_classifier() {
    // C3's premise: `gap_reason` returns from one arm, so the two phrases never
    // co-occur in one reason string. If this ever fails the two rows overlap.
    assert!(!UNRESOLVED.contains(RECEIVER_GAP));
    assert!(matches!(upstream_of("any"), Upstream::AlsoAnError));
    assert!(matches!(upstream_of("error"), Upstream::AlsoAnError));
    assert!(matches!(upstream_of("number"), Upstream::RealType));
    assert!(
        matches!(upstream_of("typeof Temporal"), Upstream::RealType),
        "a namespace object is a real type, not an error"
    );
    // The loaded label must not be the default: an unnamed identifier is
    // `EmptyName`, not `NowhereInProgram`.
    assert_eq!(bucket_for_name("", false, false, false), Bucket::EmptyName);
    assert_eq!(bucket_for_name("globalThis", false, false, false), Bucket::GlobalThis);
    assert_eq!(bucket_for_name("Temporal", true, false, false), Bucket::LibDropped);
    assert_eq!(
        bucket_for_name("globalThis", true, false, false),
        Bucket::LibDropped,
        "the lib arm is asked first, so its share is a lower bound on the others"
    );
    assert_eq!(bucket_for_name("NodeType", false, true, false), Bucket::InNamespaceBody);
    assert_eq!(bucket_for_name("q", false, false, true), Bucket::ElsewhereInProgram);
    assert_eq!(bucket_for_name("q", false, false, false), Bucket::NowhereInProgram);
}

fn upstream_of(rhs: &str) -> Upstream {
    if rhs == "any" || rhs == "error" { Upstream::AlsoAnError } else { Upstream::RealType }
}

/// The classifier proper, split out so `check_classifier` can drive it with
/// literals rather than with a program.
fn bucket_for_name(
    text: &str,
    lib_dropped: bool,
    in_namespace_body: bool,
    elsewhere: bool,
) -> Bucket {
    if text.is_empty() {
        return Bucket::EmptyName;
    }
    if lib_dropped {
        return Bucket::LibDropped;
    }
    if text == "globalThis" {
        return Bucket::GlobalThis;
    }
    if text == "arguments" {
        return Bucket::Arguments;
    }
    if matches!(text, "require" | "module" | "exports" | "__dirname" | "__filename" | "process") {
        return Bucket::CommonJsAmbient;
    }
    if in_namespace_body {
        return Bucket::InNamespaceBody;
    }
    if elsewhere {
        return Bucket::ElsewhereInProgram;
    }
    Bucket::NowhereInProgram
}

fn main() {
    check_classifier();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let reports: Vec<Report> = cases.par_iter().filter_map(measure).collect();
    let mut total = Report::default();
    for report in reports {
        total.merge(&report);
    }
    print(&total);
}

/// Names declared inside a `namespace`/`module` body, and names declared
/// anywhere at all, for this program.
fn program_names(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
) -> (HashSet<String>, HashSet<String>) {
    let mut in_namespace = HashSet::new();
    let mut anywhere = HashSet::new();
    for (_, symbol) in bound.symbols().iter() {
        anywhere.insert(symbol.name.to_string());
        let nested = symbol.declarations.iter().any(|&d| {
            let mut current = nodes.parent(d);
            while let Some(node) = current {
                if nodes.kind(node) == SyntaxKind::ModuleDeclaration {
                    return true;
                }
                current = nodes.parent(node);
            }
            false
        });
        if nested {
            in_namespace.insert(symbol.name.to_string());
        }
    }
    (in_namespace, anywhere)
}

/// Where an export marker's first declaration sits, and whether the file-level
/// exports table the checker consults actually holds the name.
fn container_of(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    symbol: tsr_binder::SymbolId,
) -> Container {
    let Some(&declaration) = bound.symbols().get(symbol).declarations.first() else {
        return Container::NoDeclaration;
    };
    let mut current = nodes.parent(declaration);
    while let Some(node) = current {
        if nodes.kind(node) == SyntaxKind::ModuleDeclaration {
            return Container::InNamespace;
        }
        current = nodes.parent(node);
    }
    Container::AtFileTop
}

/// Replays `Checker::export_symbol_of`'s two steps so the split can say *which*
/// of them failed, rather than only that the answer was `errorType`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileExport {
    /// `self.binder.symbol_of(file)` is `None` — a script file has no module
    /// symbol, so there is no exports table to read.
    NoFileSymbol,
    /// The file's exports table holds the name; `export_symbol_of` succeeded and
    /// whatever gapped is downstream of it.
    Found,
    /// A file symbol exists and its exports do not hold the name.
    Missing,
}

fn file_export_of(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    symbol: tsr_binder::SymbolId,
) -> FileExport {
    let entry = bound.symbols().get(symbol);
    let Some(&declaration) = entry.declarations.first() else { return FileExport::NoFileSymbol };
    let mut file = declaration;
    while nodes.kind(file) != SyntaxKind::SourceFile {
        match nodes.parent(file) {
            Some(parent) => file = parent,
            None => return FileExport::NoFileSymbol,
        }
    }
    let Some(module) = bound.symbol_of(file) else { return FileExport::NoFileSymbol };
    if bound.symbols().get(module).exports.contains_key(entry.name) {
        FileExport::Found
    } else {
        FileExport::Missing
    }
}

/// The symbol of the node the reason string was computed about, so the marker
/// can be re-found. Mirrors `gap_reason`'s two symbol-bearing arms.
fn symbol_of_line(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
) -> Option<tsr_binder::SymbolId> {
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = bound.symbol_of(parent)
    {
        return Some(symbol);
    }
    let Some(Node::Identifier(identifier)) = map.get(id) else { return None };
    identifier
        .node_id
        .and_then(|n| bound.resolve_name(nodes, map, n, identifier.text, SymbolFlags::VALUE))
}

#[allow(clippy::too_many_lines)]
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
    // `apply_test_directives` (`crates/tsr-conformance/src/trace_case.rs:327`)
    // reads `target`, `module`, `moduleresolution` and friends and does **not**
    // read `lib`. A case that sets `@lib` is therefore compiled against the
    // default lib set for its target, and any global its `@lib` line would have
    // brought in is absent by construction. That is bucket `LibDropped`.
    let sets_lib = parsed.options.contains_key("lib");

    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let (in_namespace, anywhere) = program_names(bound, nodes);
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));

    let mut report = Report::default();
    let name = &case.name;

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        let position_of: BTreeMap<NodeId, usize> =
            line_ids.iter().enumerate().map(|(position, id)| (*id, position)).collect();

        for (position, assertion) in our_file.iter().enumerate() {
            let baseline = expected_file.assertions.get(position);
            if assertion.type_string == "error" {
                report.gap += 1;
            } else if baseline.is_some_and(|b| b.text == assertion.line()) {
                report.right += 1;
                continue;
            } else {
                report.wrong += 1;
                continue;
            }
            let id = line_ids[position];
            let reason = types_producer::gap_reason(&mut checker, bound, nodes, map, id);

            // The `no value declaration` family, measured on the same pass.
            if reason.contains(NO_VALUE_DECL) {
                let key = reason
                    .split_once("symbol has no type: ")
                    .map_or_else(|| reason.clone(), |(_, rest)| rest.to_string());
                report.no_value_decl.entry(key.clone()).or_default().add(name, 1);
                if let Some(line) = baseline
                    && let Some(rhs) = line.text.strip_prefix(&format!("{} : ", assertion.text))
                {
                    *report.no_value_decl_rhs.entry(rhs.to_string()).or_default() += 1;
                }
                if key.starts_with("SymbolFlags(EXPORT_VALUE) /")
                    && let Some(symbol) = symbol_of_line(bound, nodes, map, id)
                {
                    let container = container_of(bound, nodes, symbol);
                    report.export_value.entry((Row::Direct, container)).or_default().add(name, 1);
                    match file_export_of(bound, nodes, symbol) {
                        FileExport::NoFileSymbol => report.file_has_no_symbol += 1,
                        FileExport::Found => report.lookup_would_hit += 1,
                        FileExport::Missing => {}
                    }
                    if let Some(line) = baseline
                        && let Some(rhs) = line.text.strip_prefix(&format!("{} : ", assertion.text))
                    {
                        *report
                            .export_value_rhs
                            .entry((container, rhs.to_string()))
                            .or_default() += 1;
                    }
                }
            }

            if reason.contains(UNRESOLVED) && reason.contains(RECEIVER_GAP) {
                report.c3_in_both_rows += 1;
            }

            // Which of the two rows, and what the root node is.
            let (row, root) = if reason.contains(UNRESOLVED) {
                report.c1_direct_total += 1;
                if !matches!(map.get(id), Some(Node::Identifier(_))) {
                    report.c1_direct_not_identifier += 1;
                }
                (Row::Direct, Some(id))
            } else if reason.contains(RECEIVER_GAP) {
                let access = if nodes.kind(id) == SyntaxKind::PropertyAccessExpression {
                    Some(id)
                } else {
                    nodes
                        .parent(id)
                        .filter(|&p| nodes.kind(p) == SyntaxKind::PropertyAccessExpression)
                };
                let Some(mut access) = access else { continue };
                // The descent from `receiver_gap.rs`, driven by the measured
                // reason at every step and never assumed.
                let (terminal, terminal_reason) = loop {
                    let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
                        break (None, String::new());
                    };
                    let Some(receiver) =
                        node.expression.as_ref().and_then(tsr_ast::Expression::node_id)
                    else {
                        break (None, String::new());
                    };
                    let reason =
                        types_producer::gap_reason(&mut checker, bound, nodes, map, receiver);
                    if reason.contains(RECEIVER_GAP)
                        && nodes.kind(receiver) == SyntaxKind::PropertyAccessExpression
                    {
                        access = receiver;
                        continue;
                    }
                    break (Some(receiver), reason);
                };
                if terminal_reason.contains(RECEIVER_GAP) {
                    report.c2_terminal_is_access += 1;
                }
                if terminal_reason.contains("SymbolFlags(EXPORT_VALUE) /")
                    && terminal_reason.contains(NO_VALUE_DECL)
                    && let Some(root) = terminal
                    && let Some(symbol) = symbol_of_line(bound, nodes, map, root)
                {
                    let container = container_of(bound, nodes, symbol);
                    report.export_value.entry((Row::Cascade, container)).or_default().add(name, 1);
                }
                if !terminal_reason.contains(UNRESOLVED) {
                    continue;
                }
                (Row::Cascade, terminal)
            } else {
                continue;
            };

            let Some(root) = root else { continue };
            let Some(Node::Identifier(identifier)) = map.get(root) else { continue };
            if row == Row::Direct {
                *report.names.entry(identifier.text.to_string()).or_default() += 1;
            }

            // C5: does a wider meaning find it? A non-zero count would make part
            // of this row a meaning bug rather than a declaration bug.
            if identifier
                .node_id
                .and_then(|n| {
                    bound.resolve_name(nodes, map, n, identifier.text, SymbolFlags::all())
                })
                .is_some()
            {
                report.c5_resolves_under_wider_meaning += 1;
            }

            let bucket = bucket_for_name(
                identifier.text,
                sets_lib,
                in_namespace.contains(identifier.text),
                anywhere.contains(identifier.text),
            );
            if bucket == Bucket::LibDropped {
                report.c4_lib_bucket_total += 1;
                if bound.globals().get(identifier.text).is_some() {
                    report.c4_lib_bucket_name_in_globals += 1;
                }
            }

            // What upstream prints for the ROOT's own line — the spellability
            // bound, and the axis the decision rule is registered on.
            let root_rhs = position_of
                .get(&root)
                .and_then(|&p| {
                    Some((expected_file.assertions.get(p)?, our_file.get(p)?.text.clone()))
                })
                .and_then(|(line, ours_text)| {
                    line.text.strip_prefix(&format!("{ours_text} : ")).map(str::to_string)
                });
            let upstream = root_rhs.as_deref().map_or(Upstream::Unknown, upstream_of);
            if let Some(rhs) = root_rhs {
                *report.spelling.entry((bucket, rhs)).or_default() += 1;
            }

            report.cells.entry((row, bucket, upstream)).or_default().add(name, 1);
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn print(t: &Report) {
    println!("gradient: right {} gap {} wrong {}", t.right, t.gap, t.wrong);

    for row in [Row::Direct, Row::Cascade] {
        let mut row_total = Tally::default();
        for ((r, _, _), tally) in &t.cells {
            if *r == row {
                row_total.merge(tally);
            }
        }
        let (cases, top1, top10) = row_total.concentration();
        println!(
            "\n================ {} — {} lines, {cases} cases, top-1 {top1:.1}%, top-10 {top10:.1}%",
            row.label(),
            row_total.lines
        );
        println!(
            "{:<52} {:>7} {:>7} {:>7} {:>6} {:>6} {:>6}",
            "bucket", "lines", "upstr", "cases", "top1", "top10", "share"
        );
        for bucket in [
            Bucket::LibDropped,
            Bucket::GlobalThis,
            Bucket::Arguments,
            Bucket::CommonJsAmbient,
            Bucket::InNamespaceBody,
            Bucket::ElsewhereInProgram,
            Bucket::NowhereInProgram,
            Bucket::EmptyName,
            Bucket::Unclassified,
        ] {
            let mut all = Tally::default();
            let mut real = Tally::default();
            for ((r, b, u), tally) in &t.cells {
                if *r != row || *b != bucket {
                    continue;
                }
                all.merge(tally);
                if *u == Upstream::RealType {
                    real.merge(tally);
                }
            }
            if all.lines == 0 {
                continue;
            }
            let (cases, top1, top10) = real.concentration();
            let share = real.lines as f64 / row_total.lines.max(1) as f64 * 100.0;
            println!(
                "{:<52} {:>7} {:>7} {:>7} {:>5.1}% {:>5.1}% {:>5.1}%",
                bucket.label(),
                all.lines,
                real.lines,
                cases,
                top1,
                top10,
                share
            );
        }
        println!("  (`upstr` = the sub-count where upstream prints a REAL type; the");
        println!("   concentration columns are taken over THAT sub-population, since it is");
        println!("   the only part a resolution fix could convert.)");
    }

    println!("\n================ THE DIRECT BUCKET THE RULE IS REGISTERED ON");
    println!("lines where the name is declared in this program AND upstream has a real type:");
    let mut decision = BTreeMap::<Bucket, Tally>::new();
    for ((_, bucket, upstream), tally) in &t.cells {
        if *upstream == Upstream::RealType
            && matches!(bucket, Bucket::InNamespaceBody | Bucket::ElsewhereInProgram)
        {
            decision.entry(*bucket).or_default().merge(tally);
        }
    }
    for (bucket, tally) in &decision {
        let (cases, top1, top10) = tally.concentration();
        println!(
            "  {:<50} {:>6} lines  {cases} cases  top-1 {top1:.1}%  top-10 {top10:.1}%",
            bucket.label(),
            tally.lines
        );
        for (case, n) in tally.top_cases(5) {
            println!("        {n:>5}  {case}");
        }
    }

    println!("\n================ UPSTREAM'S OWN ANSWER, both rows");
    let mut by_upstream = BTreeMap::<Upstream, usize>::new();
    for ((_, _, u), tally) in &t.cells {
        *by_upstream.entry(*u).or_default() += tally.lines;
    }
    let grand: usize = by_upstream.values().sum();
    for (u, n) in &by_upstream {
        println!("{n:>8}  {:>5.1}%  {}", *n as f64 / grand.max(1) as f64 * 100.0, u.label());
    }

    println!("\n================ CONTROLS");
    println!("C1 direct-row node not an Identifier  = {} (must be 0)", t.c1_direct_not_identifier);
    println!("C1' mirror: direct-row lines seen     = {}", t.c1_direct_total);
    println!("C2 terminal still a receiver gap      = {} (must be 0)", t.c2_terminal_is_access);
    println!("C3 a line in both rows                = {} (must be 0)", t.c3_in_both_rows);
    println!(
        "C4 @lib-bucket name found in globals  = {} (must be 0)",
        t.c4_lib_bucket_name_in_globals
    );
    println!("C4' mirror: @lib-bucket lines         = {}", t.c4_lib_bucket_total);
    println!(
        "C5 resolves under a wider meaning     = {} (measured, not pinned)",
        t.c5_resolves_under_wider_meaning
    );

    println!("\n================ DIRECT-ROW IDENTIFIER TEXTS (top 25)");
    for (text, n) in top(&t.names, 25) {
        println!("{n:>7}  {}", if text.is_empty() { "<empty>" } else { text.as_str() });
    }

    println!("\n================ CAN WE SPELL IT? baseline RHS of the root, per bucket");
    for bucket in [Bucket::InNamespaceBody, Bucket::ElsewhereInProgram, Bucket::LibDropped] {
        println!("-- {}", bucket.label());
        let mut rhs = BTreeMap::<String, usize>::new();
        for ((b, text), n) in &t.spelling {
            if *b == bucket {
                *rhs.entry(text.clone()).or_default() += n;
            }
        }
        for (text, n) in top(&rhs, 12) {
            println!("{n:>7}  {text}");
        }
    }

    println!("\n================ `{NO_VALUE_DECL}` FAMILY");
    let total: usize = t.no_value_decl.values().map(|v| v.lines).sum();
    println!("{total} lines");
    let mut rows: Vec<(&String, &Tally)> = t.no_value_decl.iter().collect();
    rows.sort_by_key(|(k, v)| (std::cmp::Reverse(v.lines), (*k).clone()));
    for (key, tally) in rows.into_iter().take(15) {
        let (cases, top1, top10) = tally.concentration();
        println!(
            "{:>7}  {cases:>5} cases  top-1 {top1:>5.1}%  top-10 {top10:>5.1}%  {key}",
            tally.lines
        );
        for (case, n) in tally.top_cases(4) {
            println!("            {n:>5}  {case}");
        }
    }
    println!("\n================ EXPORT_VALUE MARKERS, split by container");
    for row in [Row::Direct, Row::Cascade] {
        for container in [Container::InNamespace, Container::AtFileTop, Container::NoDeclaration] {
            let Some(tally) = t.export_value.get(&(row, container)) else { continue };
            let (cases, top1, top10) = tally.concentration();
            println!(
                "{:>7}  {cases:>4} cases  top-1 {top1:>5.1}%  top-10 {top10:>5.1}%  {} / {}",
                tally.lines,
                row.label(),
                container.label()
            );
            for (case, n) in tally.top_cases(4) {
                println!("            {n:>5}  {case}");
            }
        }
    }
    println!("  precondition: source file has NO module symbol  = {} lines", t.file_has_no_symbol);
    println!(
        "  precondition: file exports DO hold the name     = {} lines (gap is downstream)",
        t.lookup_would_hit
    );
    println!("-- baseline RHS for the InNamespace markers (top 12)");
    let mut ns_rhs = BTreeMap::<String, usize>::new();
    for ((c, text), n) in &t.export_value_rhs {
        if *c == Container::InNamespace {
            *ns_rhs.entry(text.clone()).or_default() += n;
        }
    }
    for (text, n) in top(&ns_rhs, 12) {
        println!("{n:>7}  {text}");
    }

    println!("-- what upstream prints for them (top 15)");
    for (text, n) in top(&t.no_value_decl_rhs, 15) {
        println!("{n:>7}  {text}");
    }
}

fn top<K: Clone + Ord>(m: &BTreeMap<K, usize>, n: usize) -> Vec<(K, usize)> {
    let mut v: Vec<(K, usize)> = m.iter().map(|(k, c)| (k.clone(), *c)).collect();
    v.sort_by_key(|(k, c)| (std::cmp::Reverse(*c), k.clone()));
    v.truncate(n);
    v
}
