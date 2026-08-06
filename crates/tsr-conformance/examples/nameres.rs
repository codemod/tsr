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
    /// The ALIAS row: (row, form, target) -> lines.
    alias: BTreeMap<(Row, Form, Target), Tally>,
    /// Per form, what the baseline prints for the ALIAS line itself.
    alias_rhs: BTreeMap<(Form, String), usize>,
    /// Per form, what the baseline prints for the lines a CASCADE alias
    /// unblocks — the member, not the alias. The collateral spellability check.
    alias_unblocks_rhs: BTreeMap<(Form, String), usize>,
    /// Rule 3's first spellability leg: per form, convertible lines split by
    /// whether the target was reached THROUGH an `export =` (so the printed
    /// name is the target's own) or IS the module symbol (whose name in this
    /// port is the stripped file path — `bd tsr-4jk`).
    alias_export_equals: BTreeMap<(Form, bool), Tally>,
    /// The same, restricted to the convertible lines. Leg 1 is read off this;
    /// the unrestricted map above is control C9, which must be non-zero for
    /// `export =` somewhere, because `compiler/es6ExportEqualsInterop.ts`
    /// demonstrably writes `export = Foo` five times.
    alias_export_equals_conv: BTreeMap<(Form, bool), Tally>,
    /// Rule 3's second leg: per form, the lines this alias unblocks, split by
    /// whether upstream's baseline for them contains `import(` — upstream's
    /// syntax for a symbol with no accessible name, which this port cannot
    /// produce.
    alias_unblocks_import_syntax: BTreeMap<(Form, bool), usize>,
    /// Cycle 14: (form, naming verdict) -> lines, over every line whose answer
    /// is a module object's name.
    naming: BTreeMap<(Form, Naming), Tally>,
    /// The mismatches, verbatim: (baseline RHS, what the rule predicts).
    naming_misses: BTreeMap<(String, String), usize>,
    /// The unambiguous design: (candidates == 1, verdict) -> lines.
    naming_unambiguous: BTreeMap<(bool, Naming), Tally>,
    /// C10: lines where the rule predicts a name AND the baseline is
    /// `typeof <that name>` AND the name equals the referencing alias's own
    /// name — the naive "print the local alias" rule. The gap between this and
    /// `Naming::Matches` is exactly what the earliest-declared tie-break buys.
    c10_naive_rule_agrees: usize,
    /// C8: a `NamespaceImport` with no module specifier. 0 by the grammar.
    c8_ns_import_without_specifier: usize,
    c8_ns_import_with_specifier: usize,
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
        for (k, v) in &o.alias {
            self.alias.entry(*k).or_default().merge(v);
        }
        for (k, v) in &o.alias_rhs {
            *self.alias_rhs.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.alias_unblocks_rhs {
            *self.alias_unblocks_rhs.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.alias_export_equals {
            self.alias_export_equals.entry(*k).or_default().merge(v);
        }
        for (k, v) in &o.alias_export_equals_conv {
            self.alias_export_equals_conv.entry(*k).or_default().merge(v);
        }
        for (k, v) in &o.alias_unblocks_import_syntax {
            *self.alias_unblocks_import_syntax.entry(*k).or_default() += v;
        }
        for (k, v) in &o.naming {
            self.naming.entry(*k).or_default().merge(v);
        }
        for (k, v) in &o.naming_misses {
            *self.naming_misses.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.naming_unambiguous {
            self.naming_unambiguous.entry(*k).or_default().merge(v);
        }
        self.c10_naive_rule_agrees += o.c10_naive_rule_agrees;
        self.c8_ns_import_without_specifier += o.c8_ns_import_without_specifier;
        self.c8_ns_import_with_specifier += o.c8_ns_import_with_specifier;
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
            // **The baseline is tested FIRST, and the order is the whole
            // correctness of this block.** A line where this port answers
            // `error` and upstream's baseline *also* says `error` is a **right**
            // answer, not a gap. Testing `type_string == "error"` first filed
            // 389 such lines as gaps — inherited from `receiver_gap.rs`, found
            // by `examples/reconcile.rs`, `bd tsr-zlo`. It matters twice over
            // here, because those lines are exactly the family §2 counts.
            if baseline.is_some_and(|b| b.text == assertion.line()) {
                report.right += 1;
                continue;
            } else if assertion.type_string == "error" {
                report.gap += 1;
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
                if key.starts_with("SymbolFlags(ALIAS) /")
                    && let Some(symbol) = symbol_of_line(bound, nodes, map, id)
                {
                    let (form, target, export_equals) =
                        alias_split(&mut checker, &program, bound, nodes, map, symbol);
                    report
                        .alias_export_equals
                        .entry((form, export_equals))
                        .or_default()
                        .add(name, 1);
                    if target == Target::ReachedAndTyped {
                        report
                            .alias_export_equals_conv
                            .entry((form, export_equals))
                            .or_default()
                            .add(name, 1);
                    }
                    // Cycle 14: what would the naming rule print here, and is
                    // it what upstream printed? Only asked for the forms whose
                    // target IS the module object.
                    if let Some(module) = module_target_of(&program, bound, nodes, map, symbol)
                        && let Some(line) = baseline
                        && let Some(rhs) = line.text.strip_prefix(&format!("{} : ", assertion.text))
                    {
                        let predicted = predicted_name(&program, bound, nodes, map, id, module);
                        let verdict = match (predicted, rhs.strip_prefix("typeof ")) {
                            (None, _) => Naming::NoAliasInScope,
                            (Some(_), None) => Naming::NotATypeofName,
                            (Some(p), Some(want)) if p == want => Naming::Matches,
                            (Some(p), Some(want)) => {
                                *report
                                    .naming_misses
                                    .entry((rhs.to_string(), format!("typeof {p}")))
                                    .or_default() += 1;
                                let _ = want;
                                Naming::WrongName
                            }
                        };
                        if verdict == Naming::Matches
                            && bound.symbols().get(symbol).name == predicted.unwrap_or("")
                        {
                            report.c10_naive_rule_agrees += 1;
                        }
                        report.naming.entry((form, verdict)).or_default().add(name, 1);
                        let unambiguous =
                            alias_candidate_count(&program, bound, nodes, map, id, module) == 1;
                        report
                            .naming_unambiguous
                            .entry((unambiguous, verdict))
                            .or_default()
                            .add(name, 1);
                    }
                    if form == Form::NamespaceImport {
                        let declaration = alias_declaration(bound, nodes, symbol)
                            .expect("a NamespaceImport form came from a declaration");
                        match specifier_of(nodes, map, declaration) {
                            Specifier::Absent => report.c8_ns_import_without_specifier += 1,
                            Specifier::Text(_) | Specifier::NotALiteral => {
                                report.c8_ns_import_with_specifier += 1;
                            }
                        }
                    }
                    report.alias.entry((Row::Direct, form, target)).or_default().add(name, 1);
                    if let Some(line) = baseline
                        && let Some(rhs) = line.text.strip_prefix(&format!("{} : ", assertion.text))
                    {
                        *report.alias_rhs.entry((form, rhs.to_string())).or_default() += 1;
                    }
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
                if terminal_reason.contains("SymbolFlags(ALIAS) /")
                    && terminal_reason.contains(NO_VALUE_DECL)
                    && let Some(root) = terminal
                    && let Some(symbol) = symbol_of_line(bound, nodes, map, root)
                {
                    let (form, target, export_equals) =
                        alias_split(&mut checker, &program, bound, nodes, map, symbol);
                    report
                        .alias_export_equals
                        .entry((form, export_equals))
                        .or_default()
                        .add(name, 1);
                    if target == Target::ReachedAndTyped {
                        report
                            .alias_export_equals_conv
                            .entry((form, export_equals))
                            .or_default()
                            .add(name, 1);
                    }
                    report.alias.entry((Row::Cascade, form, target)).or_default().add(name, 1);
                    // The collateral spellability check: what does the line this
                    // alias BLOCKS print? `docs/conventions.md` — the cascade
                    // runs both ways, and the sign is a naming property.
                    if let Some(line) = baseline
                        && let Some(rhs) = line.text.strip_prefix(&format!("{} : ", assertion.text))
                    {
                        *report.alias_unblocks_rhs.entry((form, rhs.to_string())).or_default() += 1;
                        *report
                            .alias_unblocks_import_syntax
                            .entry((form, rhs.contains("import(")))
                            .or_default() += 1;
                    }
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
    print_alias(t);
    print_naming(t);
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

// ===========================================================================
// Cycle 13: the `SymbolFlags(ALIAS) / no value declaration` row.
//
// 3,455 lines over 914 cases at `058b4a9`, top-1 3.8%, top-10 18.4% — the least
// concentrated large row in `docs/architecture/`. `checker-notes-symbols.md` §4
// split its 2,430-line predecessor by *form* and found 77% blocked on module
// resolution. ADR-0041 landed module resolution. **This row has never been
// measured with the seam live**, and that is what the code below does.
//
// The direct bucket, and the rule, are in §9 of
// `docs/architecture/checker-notes-nameres.md`.
// ===========================================================================

/// The alias form, taken from the **last alias-shaped declaration**, which is
/// what `Checker::declaration_of_alias_symbol` selects
/// (`getDeclarationOfAliasSymbol`, `checker.go:16397`, a `FindLast`). Reading
/// `declarations[0]` instead is a bug this port has already had and fixed.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Form {
    /// `import { x } from "./m"`. `resolve_alias` HANDLES this.
    ImportSpecifier,
    /// `export { q }` with no module specifier. HANDLED.
    ExportSpecifierLocal,
    /// `export { q } from "./m"`. HANDLED.
    ExportSpecifierFrom,
    /// `import a = b` — a bare identifier entity name. HANDLED.
    ImportEqualsIdentifier,
    /// `import d from "./m"` — the default import clause. Not handled.
    ImportClauseDefault,
    /// `import * as ns from "./m"`. Not handled; deliberately, `bd tsr-4jk`.
    NamespaceImport,
    /// `export * as ns from "./m"`. Not handled.
    NamespaceExport,
    /// `export as namespace N`. Not handled.
    NamespaceExportDeclaration,
    /// `import a = require("./m")`. Not handled.
    ImportEqualsRequire,
    /// `import a = b.c` — resolvable, deliberately not printed. Not handled.
    ImportEqualsQualified,
    /// The symbol carries `ALIAS` but has no alias-shaped declaration at all.
    /// A positive failure of the selection, not a residue.
    NoAliasDeclaration,
    /// An alias-shaped declaration whose sub-shape none of the arms above names.
    Unclassified,
}

impl Form {
    const fn label(self) -> &'static str {
        match self {
            Self::ImportSpecifier => "import { x } from 'm'      [resolve_alias HANDLES]",
            Self::ExportSpecifierLocal => "export { q }               [resolve_alias HANDLES]",
            Self::ExportSpecifierFrom => "export { q } from 'm'      [resolve_alias HANDLES]",
            Self::ImportEqualsIdentifier => "import a = b               [resolve_alias HANDLES]",
            Self::ImportClauseDefault => "import d from 'm'          (default)",
            Self::NamespaceImport => "import * as ns from 'm'    (tsr-4jk: unspellable)",
            Self::NamespaceExport => "export * as ns from 'm'",
            Self::NamespaceExportDeclaration => "export as namespace N",
            Self::ImportEqualsRequire => "import a = require('m')",
            Self::ImportEqualsQualified => "import a = b.c             (resolvable, unprintable)",
            Self::NoAliasDeclaration => "NO ALIAS DECLARATION (selection failed)",
            Self::Unclassified => "UNCLASSIFIED",
        }
    }

    /// Whether `Checker::resolve_alias` has an arm for this form today. A line
    /// under a handled form is **not** a resolution gap: the target was reached
    /// and its own type gapped, which makes it kind 2 and someone else's row.
    const fn is_handled(self) -> bool {
        matches!(
            self,
            Self::ImportSpecifier
                | Self::ExportSpecifierLocal
                | Self::ExportSpecifierFrom
                | Self::ImportEqualsIdentifier
        )
    }
}

/// Whether the target could be reached *and* typed, replayed independently of
/// `Checker::resolve_alias` (which is private) through the same seam it uses.
///
/// **This is the direct bucket the §9 rule is registered on.** It is not a
/// proxy: it asks the question the decision is about — *if this alias resolved,
/// would this port have an answer?* — rather than a quantity derived from it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Target {
    /// The target was reached and `get_type_of_symbol` gives it a real type.
    /// **Convertible.**
    ReachedAndTyped,
    /// The target was reached and its own type is `errorType`. Kind 2: blocked
    /// on whatever types *that* symbol, not on alias resolution.
    ReachedButUntyped,
    /// The specifier named a file the program does not hold, or that file has
    /// no module symbol.
    ModuleNotResolved,
    /// The module resolved but does not export the name.
    NameNotExported,
    /// This probe has no replay for the form.
    NotReplayed,
}

impl Target {
    const fn label(self) -> &'static str {
        match self {
            Self::ReachedAndTyped => "reached AND typed  <- convertible",
            Self::ReachedButUntyped => "reached, type gaps (kind 2)",
            Self::ModuleNotResolved => "module not resolved",
            Self::NameNotExported => "module resolved, name not exported",
            Self::NotReplayed => "no replay for this form",
        }
    }
}

/// Form and target for one ALIAS-row symbol, so the direct and cascade branches
/// cannot classify the same symbol two different ways.
fn alias_split(
    checker: &mut tsr_checker::Checker<'_, '_>,
    program: &tsr_compiler::Program<'_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    symbol: tsr_binder::SymbolId,
) -> (Form, Target, bool) {
    let Some(declaration) = alias_declaration(bound, nodes, symbol) else {
        return (Form::NoAliasDeclaration, Target::NotReplayed, false);
    };
    let form = form_of(nodes, map, declaration);
    // The name a specifier looks up is its `propertyName` when it has one —
    // `import { a as b }` looks up `a`, not `b` (`checker.go:14677`).
    let lookup = match map.get(declaration) {
        Some(Node::ImportSpecifier(node)) => match node.property_name {
            Some(tsr_ast::ModuleExportName::Identifier(name)) => name.text,
            _ => bound.symbols().get(symbol).name,
        },
        Some(Node::ExportSpecifier(node)) => match node.property_name {
            Some(tsr_ast::ModuleExportName::Identifier(name)) => name.text,
            _ => bound.symbols().get(symbol).name,
        },
        Some(Node::ImportEqualsDeclaration(node)) => match node.module_reference {
            Some(tsr_ast::ModuleReference::Identifier(name)) => name.text,
            _ => bound.symbols().get(symbol).name,
        },
        _ => bound.symbols().get(symbol).name,
    };
    let target = target_of(checker, program, bound, nodes, map, declaration, form, lookup);
    (form, target, reaches_through_export_equals(program, bound, nodes, map, declaration))
}

/// Whether the module this form names writes `export = X`.
///
/// The whole spellability question for `import a = require("m")` and
/// `import * as ns from "m"`: with an `export =`, `resolve_external_module_symbol`
/// (`checker.go:15556`) hands back **`X`**, a declared symbol with its own name,
/// and `typeof React` is printable. Without one it hands back the **module
/// symbol**, whose name in this port is the stripped file path, and the same
/// line prints `typeof /moduleA` — `bd tsr-4jk`'s finding, and the reason two
/// designs were refused this week at 2.1 and 2.5 wrong per right.
fn reaches_through_export_equals(
    program: &tsr_compiler::Program<'_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    declaration: NodeId,
) -> bool {
    let Specifier::Text(specifier) = specifier_of(nodes, map, declaration) else { return false };
    let mut file = declaration;
    while nodes.kind(file) != SyntaxKind::SourceFile {
        let Some(parent) = nodes.parent(file) else { return false };
        file = parent;
    }
    let Some(target_file) = program.resolved_module(file, specifier) else { return false };
    let Some(module) = bound.symbol_of(target_file) else { return false };
    bound.symbols().get(module).exports.contains_key("export=")
}

/// `Checker::declaration_of_alias_symbol` replayed: `FindLast` over the
/// alias-shaped kinds (`ast/utilities.go:2631`).
fn alias_declaration(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    symbol: tsr_binder::SymbolId,
) -> Option<NodeId> {
    bound.symbols().get(symbol).declarations.iter().rev().copied().find(|&declaration| {
        matches!(
            nodes.kind(declaration),
            SyntaxKind::ImportEqualsDeclaration
                | SyntaxKind::NamespaceExportDeclaration
                | SyntaxKind::NamespaceImport
                | SyntaxKind::NamespaceExport
                | SyntaxKind::ImportSpecifier
                | SyntaxKind::ExportSpecifier
                | SyntaxKind::ImportClause
        )
    })
}

/// Where a form's module specifier is, and what it says.
///
/// **The three answers must stay apart**, and conflating two of them fired
/// control C8 on the first run: `Absent` is the grammar claim (an `import * as
/// ns` is a clause of an `ImportDeclaration`, which cannot exist without a
/// specifier, so `Absent` must be **0**), while `NotALiteral` is parser error
/// recovery and is allowed to be non-zero. A single `Option<&str>` reported both
/// as "no specifier" and made a control read 2 where it must read 0 — the
/// control was right and the probe was wrong.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Specifier<'a> {
    /// A string literal specifier, with its text.
    Text(&'a str),
    /// A specifier node that is not a string literal. Parser recovery.
    NotALiteral,
    /// No specifier node at all.
    Absent,
}

/// The module specifier that governs `id`.
///
/// Two shapes, and the second is why the first run reported **0 convertible for
/// all 2,014 `import a = require("m")` lines**: that form's specifier is the
/// argument of the `require(...)` on the declaration itself, not a
/// `module_specifier` on any ancestor, so a parent walk finds nothing and every
/// line was filed `ModuleNotResolved`. That was a defect in this probe, not a
/// finding about the port.
fn specifier_of<'a>(
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'a>,
    id: NodeId,
) -> Specifier<'a> {
    let literal = |expression: Option<tsr_ast::Expression<'a>>| match expression {
        Some(tsr_ast::Expression::StringLiteral(text)) => Specifier::Text(text.text),
        Some(_) => Specifier::NotALiteral,
        None => Specifier::Absent,
    };
    // `import a = require("m")` — the specifier is on the declaration.
    if let Some(Node::ImportEqualsDeclaration(node)) = map.get(id)
        && let Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) =
            node.module_reference
    {
        return literal(reference.expression);
    }
    let mut current = Some(id);
    for _ in 0..4 {
        let Some(node) = current else { return Specifier::Absent };
        match map.get(node) {
            Some(Node::ImportDeclaration(declaration)) => {
                return literal(declaration.module_specifier);
            }
            Some(Node::ExportDeclaration(declaration)) => {
                return literal(declaration.module_specifier);
            }
            _ => {}
        }
        current = nodes.parent(node);
    }
    Specifier::Absent
}

fn form_of(nodes: &tsr_ast::NodeTable, map: &tsr_ast::NodeMap<'_>, declaration: NodeId) -> Form {
    match nodes.kind(declaration) {
        SyntaxKind::ImportSpecifier => Form::ImportSpecifier,
        SyntaxKind::ExportSpecifier => {
            // The grammar question — does the `export { q }` have a `from`? —
            // and NOT whether the specifier is a string literal. Those are
            // different, and `getTargetOfExportSpecifier` (`checker.go:14951`)
            // branches on the first.
            if specifier_of(nodes, map, declaration) == Specifier::Absent {
                Form::ExportSpecifierLocal
            } else {
                Form::ExportSpecifierFrom
            }
        }
        SyntaxKind::ImportClause => Form::ImportClauseDefault,
        SyntaxKind::NamespaceImport => Form::NamespaceImport,
        SyntaxKind::NamespaceExport => Form::NamespaceExport,
        SyntaxKind::NamespaceExportDeclaration => Form::NamespaceExportDeclaration,
        SyntaxKind::ImportEqualsDeclaration => match map.get(declaration) {
            Some(Node::ImportEqualsDeclaration(node)) => match node.module_reference {
                Some(tsr_ast::ModuleReference::Identifier(_)) => Form::ImportEqualsIdentifier,
                Some(tsr_ast::ModuleReference::QualifiedName(_)) => Form::ImportEqualsQualified,
                Some(tsr_ast::ModuleReference::ExternalModuleReference(_)) => {
                    Form::ImportEqualsRequire
                }
                None => Form::Unclassified,
            },
            _ => Form::Unclassified,
        },
        _ => Form::Unclassified,
    }
}

/// Replay the target lookup through the same seam `resolve_alias` uses, then ask
/// the checker whether that symbol has a type.
///
/// Deliberately **not** a call into `resolve_alias`: that function declines four
/// forms on purpose, so asking it would answer "no" for exactly the population
/// under measurement. This asks what is *available*, which is the question.
#[allow(clippy::too_many_arguments)]
fn target_of(
    checker: &mut tsr_checker::Checker<'_, '_>,
    program: &tsr_compiler::Program<'_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    declaration: NodeId,
    form: Form,
    name: &str,
) -> Target {
    let error = checker.intrinsics().error;
    let verdict = |checker: &mut tsr_checker::Checker<'_, '_>, symbol| {
        if checker.get_type_of_symbol(symbol) == error {
            Target::ReachedButUntyped
        } else {
            Target::ReachedAndTyped
        }
    };

    // The module symbol a specifier names, plus `export =` applied — the two
    // steps `get_external_module_member` takes before the table lookup.
    let module_symbol = |bound: &tsr_binder::BindResult<'_>| -> Option<tsr_binder::SymbolId> {
        let Specifier::Text(specifier) = specifier_of(nodes, map, declaration) else { return None };
        let mut file = declaration;
        while nodes.kind(file) != SyntaxKind::SourceFile {
            file = nodes.parent(file)?;
        }
        let target_file = program.resolved_module(file, specifier)?;
        let module = bound.symbol_of(target_file)?;
        Some(bound.symbols().get(module).exports.get("export=").copied().unwrap_or(module))
    };

    match form {
        Form::NamespaceImport | Form::NamespaceExport | Form::ImportEqualsRequire => {
            match module_symbol(bound) {
                Some(module) => verdict(checker, module),
                None => Target::ModuleNotResolved,
            }
        }
        Form::ImportClauseDefault => {
            let Some(module) = module_symbol(bound) else { return Target::ModuleNotResolved };
            match bound.symbols().get(module).exports.get("default").copied() {
                Some(target) => verdict(checker, target),
                None => Target::NameNotExported,
            }
        }
        Form::ImportSpecifier | Form::ExportSpecifierFrom => {
            let Some(module) = module_symbol(bound) else { return Target::ModuleNotResolved };
            match bound.symbols().get(module).exports.get(name).copied() {
                Some(target) => verdict(checker, target),
                None => Target::NameNotExported,
            }
        }
        Form::ExportSpecifierLocal | Form::ImportEqualsIdentifier | Form::ImportEqualsQualified => {
            match bound.resolve_name(nodes, map, declaration, name, SymbolFlags::all()) {
                Some(target) => verdict(checker, target),
                None => Target::NameNotExported,
            }
        }
        Form::NamespaceExportDeclaration | Form::NoAliasDeclaration | Form::Unclassified => {
            Target::NotReplayed
        }
    }
}

const FORMS: [Form; 12] = [
    Form::ImportSpecifier,
    Form::ExportSpecifierLocal,
    Form::ExportSpecifierFrom,
    Form::ImportEqualsIdentifier,
    Form::ImportClauseDefault,
    Form::NamespaceImport,
    Form::NamespaceExport,
    Form::NamespaceExportDeclaration,
    Form::ImportEqualsRequire,
    Form::ImportEqualsQualified,
    Form::NoAliasDeclaration,
    Form::Unclassified,
];

const TARGETS: [Target; 5] = [
    Target::ReachedAndTyped,
    Target::ReachedButUntyped,
    Target::ModuleNotResolved,
    Target::NameNotExported,
    Target::NotReplayed,
];

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn print_alias(t: &Report) {
    println!("\n================ SymbolFlags(ALIAS) / no value declaration");

    let mut grand = Tally::default();
    for tally in t.alias.values() {
        grand.merge(tally);
    }
    let (cases, top1, top10) = grand.concentration();
    println!(
        "{} lines (direct + cascade), {cases} cases, top-1 {top1:.1}%, top-10 {top10:.1}%",
        grand.lines
    );
    for (case, n) in grand.top_cases(5) {
        println!("      {n:>5}  {case}");
    }

    println!("\n-- by form (both rows), and how many are CONVERTIBLE --");
    println!(
        "{:<52} {:>7} {:>7} {:>7} {:>7} {:>6} {:>6}",
        "form", "direct", "cascade", "total", "conv", "cases", "top1"
    );
    for form in FORMS {
        let mut direct = Tally::default();
        let mut cascade = Tally::default();
        let mut convertible = Tally::default();
        for ((row, f, target), tally) in &t.alias {
            if *f != form {
                continue;
            }
            match row {
                Row::Direct => direct.merge(tally),
                Row::Cascade => cascade.merge(tally),
            }
            if *target == Target::ReachedAndTyped {
                convertible.merge(tally);
            }
        }
        let total = direct.lines + cascade.lines;
        if total == 0 {
            continue;
        }
        let (cases, top1, _) = convertible.concentration();
        println!(
            "{:<52} {:>7} {:>7} {:>7} {:>7} {:>6} {:>5.1}%",
            form.label(),
            direct.lines,
            cascade.lines,
            total,
            convertible.lines,
            cases,
            top1
        );
    }

    println!("\n-- by target verdict (both rows), the direct bucket --");
    let mut by_target = BTreeMap::<Target, Tally>::new();
    for ((_, _, target), tally) in &t.alias {
        by_target.entry(*target).or_default().merge(tally);
    }
    for target in TARGETS {
        let Some(tally) = by_target.get(&target) else { continue };
        let (cases, top1, top10) = tally.concentration();
        println!(
            "{:>7}  {:>5.1}%  {cases:>4} cases  top-1 {top1:>5.1}%  top-10 {top10:>5.1}%  {}",
            tally.lines,
            tally.lines as f64 / grand.lines.max(1) as f64 * 100.0,
            target.label()
        );
    }

    println!("\n-- THE DECISION BUCKET: convertible, split handled/unhandled --");
    for handled in [false, true] {
        let mut tally = Tally::default();
        for ((_, form, target), t2) in &t.alias {
            if *target == Target::ReachedAndTyped && form.is_handled() == handled {
                tally.merge(t2);
            }
        }
        let (cases, top1, top10) = tally.concentration();
        println!(
            "{:>7} lines  {cases:>4} cases  top-1 {top1:>5.1}%  top-10 {top10:>5.1}%  {}",
            tally.lines,
            if handled {
                "under a form resolve_alias ALREADY handles (not resolution work)"
            } else {
                "under a form resolve_alias does NOT handle  <- THE RULE'S POPULATION"
            }
        );
        for (case, n) in tally.top_cases(5) {
            println!("          {n:>5}  {case}");
        }
    }

    println!("\n-- C9: `export =` over ALL alias lines (must be non-zero somewhere) --");
    println!("{:<52} {:>10} {:>10} {:>8}", "form", "export=", "bare module", "export=%");
    for form in FORMS {
        let yes = t.alias_export_equals.get(&(form, true)).map_or(0, |t2| t2.lines);
        let no = t.alias_export_equals.get(&(form, false)).map_or(0, |t2| t2.lines);
        if yes + no == 0 {
            continue;
        }
        println!(
            "{:<52} {:>10} {:>10} {:>7.1}%",
            form.label(),
            yes,
            no,
            yes as f64 / (yes + no) as f64 * 100.0
        );
    }

    println!("\n-- RULE 3 LEG 1: CONVERTIBLE lines, does the target come through `export =`? --");
    println!("{:<52} {:>10} {:>10} {:>8}", "form", "export=", "bare module", "export=%");
    for form in FORMS {
        let yes = t.alias_export_equals_conv.get(&(form, true)).map_or(0, |t2| t2.lines);
        let no = t.alias_export_equals_conv.get(&(form, false)).map_or(0, |t2| t2.lines);
        if yes + no == 0 {
            continue;
        }
        println!(
            "{:<52} {:>10} {:>10} {:>7.1}%",
            form.label(),
            yes,
            no,
            yes as f64 / (yes + no) as f64 * 100.0
        );
    }

    println!("\n-- RULE 3 LEG 2: lines this form UNBLOCKS whose baseline uses `import(` --");
    println!("{:<52} {:>10} {:>10} {:>8}", "form", "import(", "nameable", "import(%");
    for form in FORMS {
        let yes = t.alias_unblocks_import_syntax.get(&(form, true)).copied().unwrap_or(0);
        let no = t.alias_unblocks_import_syntax.get(&(form, false)).copied().unwrap_or(0);
        if yes + no == 0 {
            continue;
        }
        println!(
            "{:<52} {:>10} {:>10} {:>7.1}%",
            form.label(),
            yes,
            no,
            yes as f64 / (yes + no) as f64 * 100.0
        );
    }

    println!("\n-- CAN WE SPELL THE ROW? baseline RHS of the alias line, per form --");
    for form in FORMS {
        let mut rhs = BTreeMap::<String, usize>::new();
        for ((f, text), n) in &t.alias_rhs {
            if *f == form {
                *rhs.entry(text.clone()).or_default() += n;
            }
        }
        if rhs.is_empty() {
            continue;
        }
        println!("  -- {}", form.label());
        for (text, n) in top(&rhs, 8) {
            println!("  {n:>7}  {text}");
        }
    }

    println!("\n-- CAN WE SPELL WHAT IT UNBLOCKS? baseline RHS of the BLOCKED line --");
    for form in FORMS {
        let mut rhs = BTreeMap::<String, usize>::new();
        for ((f, text), n) in &t.alias_unblocks_rhs {
            if *f == form {
                *rhs.entry(text.clone()).or_default() += n;
            }
        }
        if rhs.is_empty() {
            continue;
        }
        println!("  -- {}", form.label());
        for (text, n) in top(&rhs, 8) {
            println!("  {n:>7}  {text}");
        }
    }

    println!(
        "\nC8 NamespaceImport with no module specifier = {} (must be 0), mirror {}",
        t.c8_ns_import_without_specifier, t.c8_ns_import_with_specifier
    );
}

// ===========================================================================
// Cycle 14: can a module-symbol-typed reference be given the name the corpus
// asks for?
//
// Every refusal in this workstream bottoms out in one sentence — this port
// cannot spell a module symbol's name, because `TypeData::Anonymous`'s `text`
// is baked at type creation and a module symbol's name here is the stripped
// file path. `bd tsr-6j2`.
//
// Upstream picks the name at the REFERENCE site, not from the declaration.
// `NodeBuilderImpl.lookupSymbolChain` (`internal/checker/nodebuilderimpl.go:1061`)
// -> `getSymbolChain` -> `Checker.getAccessibleSymbolChain`
// (`internal/checker/symbolaccessibility.go:373`) -> `trySymbolTable`, which
// iterates the **alias symbols of every table in scope**, keeps each one that
// resolves to the target, and then:
//
//     slices.SortStableFunc(candidateChains, c.compareSymbolChains)
//     return candidateChains[0]                       // "pick first, shortest"
//
// `compareSymbolsWorker` (`internal/checker/utilities.go:366`) breaks the tie on
// `compareNodes(s1.Declarations[0], s2.Declarations[0])`, which is **file index
// in the program, then source position**. So:
//
//   **among the aliases in scope that resolve to this module, the
//   EARLIEST-DECLARED one supplies the name.**
//
// That single rule explains both counter-examples `tsr-6j2` records, which are
// the same mechanism twice:
//
//   compiler/es6ImportNameSpaceImport  — `nameSpaceBinding` and
//     `nameSpaceBinding2` both alias `./es6ImportNameSpaceImport_0`;
//     `nameSpaceBinding2`'s own declaration line prints `typeof nameSpaceBinding`.
//   compiler/modulePreserve4 — `import g1 from "./g"` at line 154 and
//     `import g2 = require("./g")` at line 162; `g2` prints `typeof g1`.
//
// This section measures whether that rule reproduces the corpus, and how many
// lines it releases. It builds nothing.
// ===========================================================================

/// Whether the rule's predicted name matches what upstream printed.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Naming {
    /// The rule predicts exactly the baseline's right-hand side.
    Matches,
    /// The baseline is `typeof X` and the rule predicts a different `X`.
    WrongName,
    /// The baseline is not of the form `typeof <identifier>` at all.
    NotATypeofName,
    /// No alias in scope resolves to this module, so the rule predicts nothing.
    NoAliasInScope,
}

impl Naming {
    const fn label(self) -> &'static str {
        match self {
            Self::Matches => "rule predicts the baseline exactly",
            Self::WrongName => "baseline is `typeof X`, rule predicts a different X",
            Self::NotATypeofName => "baseline is not `typeof <identifier>`",
            Self::NoAliasInScope => "no alias in scope resolves to this module",
        }
    }
}

/// Every alias symbol visible from `start`, in the scope order
/// `Binder::resolve_name` walks, plus the globals.
///
/// This is `someSymbolTableInScope` (`symbolaccessibility.go`) reduced to the
/// two tables this port has: a scope's `locals`, and the program's globals.
/// Members tables are skipped, which upstream also does — `getSymbolTableAliases`
/// returns nothing for them because "members tables never contain alias symbols".
fn aliases_in_scope(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    start: NodeId,
) -> Vec<tsr_binder::SymbolId> {
    let mut found = Vec::new();
    let mut current = Some(start);
    while let Some(node) = current {
        if let Some(locals) = bound.locals(node) {
            for symbol in locals.values() {
                if bound.symbols().get(*symbol).flags.intersects(SymbolFlags::ALIAS) {
                    found.push(*symbol);
                }
            }
        }
        current = nodes.parent(node);
    }
    for symbol in bound.globals().values() {
        if bound.symbols().get(*symbol).flags.intersects(SymbolFlags::ALIAS) {
            found.push(*symbol);
        }
    }
    found
}

/// The module symbol an alias names, for the three forms whose target **is** the
/// module object rather than one of its exports.
///
/// Restricted on purpose. `import d from "m"` resolves to `exports["default"]`,
/// a declared symbol with its own name, and is not part of the naming problem;
/// including it would inflate the release count with lines that were never
/// blocked on this.
fn module_target_of(
    program: &tsr_compiler::Program<'_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    symbol: tsr_binder::SymbolId,
) -> Option<tsr_binder::SymbolId> {
    let declaration = alias_declaration(bound, nodes, symbol)?;
    if !matches!(
        form_of(nodes, map, declaration),
        Form::NamespaceImport | Form::NamespaceExport | Form::ImportEqualsRequire
    ) {
        return None;
    }
    let Specifier::Text(specifier) = specifier_of(nodes, map, declaration) else { return None };
    let mut file = declaration;
    while nodes.kind(file) != SyntaxKind::SourceFile {
        file = nodes.parent(file)?;
    }
    let target_file = program.resolved_module(file, specifier)?;
    let module = bound.symbol_of(target_file)?;
    // `resolveExternalModuleSymbol` (`checker.go:15556`): a module writing
    // `export = X` **is** `X`, and `X` has a name of its own — so it is not part
    // of the naming problem and is excluded here rather than counted as a win.
    if bound.symbols().get(module).exports.contains_key("export=") {
        return None;
    }
    Some(module)
}

/// The name upstream would print for `module` at `start`: the earliest-declared
/// alias in scope that resolves to it.
///
/// The ordering is `compareNodes` (`internal/checker/utilities.go:366` ->
/// `:392`): file index in the program, then source position. One shared
/// `NodeTable` spans the program in parse order (ADR-0034), so a `NodeId`
/// already orders files; the span start orders within one file. Both are used,
/// in that order, rather than relying on `NodeId` alone — a node is registered
/// after its children, so ids are creation order and not source order.
fn predicted_name<'a>(
    program: &tsr_compiler::Program<'_>,
    bound: &tsr_binder::BindResult<'a>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'a>,
    start: NodeId,
    module: tsr_binder::SymbolId,
) -> Option<&'a str> {
    let mut best: Option<(usize, u32, &str)> = None;
    for candidate in aliases_in_scope(bound, nodes, start) {
        if module_target_of(program, bound, nodes, map, candidate) != Some(module) {
            continue;
        }
        let Some(&declaration) = bound.symbols().get(candidate).declarations.first() else {
            continue;
        };
        let file_index = program
            .source_files()
            .iter()
            .position(|file| file.node_range().contains(&declaration.as_u32()))
            .unwrap_or(usize::MAX);
        let key = (file_index, nodes.span(declaration).start);
        let name = bound.symbols().get(candidate).name;
        if best.is_none_or(|(f, p, _)| (key.0, key.1) < (f, p)) {
            best = Some((key.0, key.1, name));
        }
    }
    best.map(|(_, _, name)| name)
}

/// How many distinct aliases in scope resolve to `module`.
///
/// The design knob. When it is **1** the name is forced — every rule anyone
/// could write agrees — and printing it is safe. When it is **2 or more** the
/// corpus contradicts itself: `compiler/es6ImportNameSpaceImport` prints the
/// *earliest* alias for a later one, and
/// `compiler/unusedImports_entireImportDeclaration` prints each of `ns`, `ns2`,
/// `ns3` under its *own* name. Both are two-plus-alias scopes and they disagree,
/// so no single tie-break reproduces both.
///
/// That is what makes a gap possible here at all: ambiguity is detectable at the
/// reference site, before anything is printed.
fn alias_candidate_count(
    program: &tsr_compiler::Program<'_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    start: NodeId,
    module: tsr_binder::SymbolId,
) -> usize {
    let mut names = std::collections::BTreeSet::<&str>::new();
    for candidate in aliases_in_scope(bound, nodes, start) {
        if module_target_of(program, bound, nodes, map, candidate) == Some(module) {
            names.insert(bound.symbols().get(candidate).name);
        }
    }
    names.len()
}

#[allow(clippy::cast_precision_loss)]
fn print_naming(t: &Report) {
    println!("\n================ CYCLE 14: does the earliest-declared-alias rule work?");
    let mut grand = Tally::default();
    for tally in t.naming.values() {
        grand.merge(tally);
    }
    let (cases, top1, top10) = grand.concentration();
    println!(
        "{} lines whose answer is a module object's name, {cases} cases, top-1 {top1:.1}%, top-10 {top10:.1}%",
        grand.lines
    );

    let mut by_verdict = BTreeMap::<Naming, Tally>::new();
    for ((_, verdict), tally) in &t.naming {
        by_verdict.entry(*verdict).or_default().merge(tally);
    }
    for verdict in
        [Naming::Matches, Naming::WrongName, Naming::NotATypeofName, Naming::NoAliasInScope]
    {
        let Some(tally) = by_verdict.get(&verdict) else { continue };
        let (cases, top1, _) = tally.concentration();
        println!(
            "{:>7}  {:>5.1}%  {cases:>4} cases  top-1 {top1:>5.1}%  {}",
            tally.lines,
            tally.lines as f64 / grand.lines.max(1) as f64 * 100.0,
            verdict.label()
        );
    }

    println!("\n-- by form --");
    for form in FORMS {
        let mut row = BTreeMap::<Naming, usize>::new();
        for ((f, verdict), tally) in &t.naming {
            if *f == form {
                *row.entry(*verdict).or_default() += tally.lines;
            }
        }
        let total: usize = row.values().sum();
        if total == 0 {
            continue;
        }
        println!(
            "{:<52} {:>7} total  {:>7} match ({:>5.1}%)  {:>5} wrong-name  {:>5} other",
            form.label(),
            total,
            row.get(&Naming::Matches).copied().unwrap_or(0),
            row.get(&Naming::Matches).copied().unwrap_or(0) as f64 / total as f64 * 100.0,
            row.get(&Naming::WrongName).copied().unwrap_or(0),
            row.get(&Naming::NotATypeofName).copied().unwrap_or(0)
                + row.get(&Naming::NoAliasInScope).copied().unwrap_or(0)
        );
    }

    println!(
        "\nC10 lines the NAIVE `print the local alias` rule also gets right = {}",
        t.c10_naive_rule_agrees
    );
    println!(
        "    (the earliest-declared tie-break is worth {} lines on top; if this equals",
        by_verdict
            .get(&Naming::Matches)
            .map_or(0, |v| v.lines)
            .saturating_sub(t.c10_naive_rule_agrees)
    );
    println!("     the match count, the two known counter-examples are the whole residue)");

    println!("\n-- THE DESIGN KNOB: print only when EXACTLY ONE alias in scope resolves --");
    for unambiguous in [true, false] {
        let mut row = BTreeMap::<Naming, usize>::new();
        for ((u, verdict), tally) in &t.naming_unambiguous {
            if *u == unambiguous {
                *row.entry(*verdict).or_default() += tally.lines;
            }
        }
        let total: usize = row.values().sum();
        if total == 0 {
            continue;
        }
        let matches = row.get(&Naming::Matches).copied().unwrap_or(0);
        let wrong = row.get(&Naming::WrongName).copied().unwrap_or(0);
        println!(
            "  {:<28} {total:>5} lines  {matches:>5} match  {wrong:>4} wrong-name  {:>4} other",
            if unambiguous { "exactly 1 alias  -> PRINT" } else { "2 or more        -> GAP" },
            total - matches - wrong
        );
        let mut tally = Tally::default();
        for ((u, _), t2) in &t.naming_unambiguous {
            if *u == unambiguous {
                tally.merge(t2);
            }
        }
        let (cases, top1, top10) = tally.concentration();
        println!("      {cases} cases, top-1 {top1:.1}%, top-10 {top10:.1}%");
    }

    println!("\n-- THE RESIDUE, verbatim: baseline vs what the rule predicts (top 20) --");
    for ((want, got), n) in top(&t.naming_misses, 20) {
        println!("{n:>7}  baseline `{want}`  rule `{got}`");
    }
}
