//! What actually gaps the receiver, for the two `the receiver is a gap` rows.
//!
//! # The claim under test
//!
//! `bd tsr-6ph` says a type for the module object is **"the only route to"** the
//! 1,590 lines in `member name, the receiver is a gap` (736 + 60) and
//! `property access, the receiver is a gap` (734 + 60), *"because `ns.foo` needs
//! the namespace's type and not the alias's target"*.
//!
//! That is a claim of the form *X is the only route to Y*, which excludes
//! alternatives, and `docs/conventions.md` records eight wrong claims here that
//! came from stopping when the evidence supported a conclusion rather than when
//! it excluded the alternatives. The two rows say **that** the receiver gapped;
//! they do not say **which** gap gapped it. This probe walks the second step.
//!
//! # What it does
//!
//! `types_producer::access_reason` already emits the receiver's node kind in the
//! reason string, and `gap_reason` is `pub`. So no new checker machinery is
//! needed: for every gap line whose reason carries `the receiver is a gap`, take
//! the access, take its receiver, and ask `gap_reason` **about the receiver**.
//! While the answer is itself `the receiver is a gap` — `a.b.c` — descend again.
//! The descent is driven by the measured reason at each step, never assumed, so
//! a chain that stops gapping for a different cause stops the walk there.
//!
//! The terminal receiver is then classified by what declares it. The buckets are
//! the alternatives `tsr-6ph`'s claim has to exclude:
//!
//! - `import * as ns` / `export * as ns` / `import a = require(...)` — the
//!   namespace forms. These are the ones the module-object type would answer.
//! - an ordinary imported binding (`import { x }`, `import d from`) — a
//!   cross-file alias, but one whose answer is an **export symbol**, not a
//!   module object. `tsr-6ph` does not cover these.
//! - a local declaration whose own type gaps — not module-related at all.
//! - a name that does not resolve.
//! - a receiver that is not an identifier at all (a call, `this`, a literal).
//!
//! # Controls, printed unconditionally
//!
//! Three of the four are pinned by **construction** rather than by arithmetic
//! (`docs/conventions.md`), which is what lets them see a semantic inversion
//! that leaves every sum intact:
//!
//! - **C1 — the phrase reaches exactly two rows.** `access_reason` has two call
//!   sites in `types_producer` (`member name,` and `property access,`), so a
//!   gap line carrying `the receiver is a gap` under any third row prefix is
//!   **0 before this file was written**. It reads non-zero if the row matcher
//!   drifts from the producer.
//! - **C2 — the walk terminates outside a property access.** The descent
//!   continues only while the node is a `PropertyAccessExpression`, and
//!   `access_reason` is reachable from no other node kind, so the count of
//!   terminal reasons still carrying `the receiver is a gap` is **0 by
//!   construction**. It reads non-zero if the descent silently stops early.
//! - **C3 — a namespace import always has a module specifier.** By the grammar,
//!   `import * as ns` is a clause of an `ImportDeclaration`, which cannot exist
//!   without one. The count of `import * as ns` roots from which no specifier
//!   could be read is **0**, and the mirror count is printed beside it so the
//!   pair pins the classifier rather than one arm of it.
//! - **A1 — the buckets partition the rows.** Arithmetic. `UNKNOWN` is a real
//!   arm with a positive test (a node the map does not hold), not the default
//!   that absorbs everything unmatched — `docs/conventions.md`, *"a control
//!   bucket over a classifier whose last arm is a default cannot fire"*.
//!
//! # Can we spell the answer?
//!
//! `bd tsr-4jk` measured that upstream prints the **local alias** for a
//! namespace import — `>ns : typeof ns` — never the module, and a module
//! symbol's name in this port is the stripped file path, so a naive
//! implementation prints `typeof /0`: a gap converted into a *wrong* line. So
//! for the terminal receiver this probe also prints the **baseline's own**
//! right-hand side, per bucket. That is the check `docs/conventions.md` calls
//! *"one grep at the baseline's right-hand side"*, and it bounds every bucket
//! before any line of checker is written.
//!
//! Run: `cargo run --release -p tsr-conformance --example receiver_gap`

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The phrase both rows are named after, verbatim from
/// `types_producer::access_reason`.
const PHRASE: &str = "the receiver is a gap";

/// The two row prefixes `access_reason` is reachable under. Control C1 is the
/// count of lines carrying [`PHRASE`] under neither.
const ROW_MEMBER: &str = "member name, ";
const ROW_ACCESS: &str = "property access, ";

/// What declares the receiver at the bottom of the chain.
///
/// Every arm carries a positive test. [`Bucket::Unknown`] is reached only when
/// the node map does not hold the node — a real failure of the walk, not the
/// residue of an `else`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Bucket {
    /// `import * as ns from "m"`. The module-object form `tsr-6ph` is about.
    NamespaceImport,
    /// `import a = require("m")` and `import a = b.c`. Also `tsr-6ph`.
    ImportEquals,
    /// `export * as ns from "m"`. Also `tsr-6ph`.
    NamespaceExport,
    /// `import { x }` / `import d from` / `export { x } from`. A cross-file
    /// alias whose answer is an export symbol, **not** a module object.
    OtherAlias,
    /// A `namespace N {}` / `module N {}` declared in this program. Needs a type
    /// for a namespace, but not a *module* and not the cross-file seam.
    LocalNamespace,
    /// Any other local declaration whose own type gaps. Not module-related.
    LocalDeclaration,
    /// The name resolved to nothing.
    Unresolved,
    /// The receiver is not an identifier: a call, `this`, an element access.
    NotAnIdentifier,
    /// The node map does not hold the node. A positive failure of the walk.
    Unknown,
}

impl Bucket {
    const fn label(self) -> &'static str {
        match self {
            Self::NamespaceImport => "import * as ns          (tsr-6ph)",
            Self::ImportEquals => "import a = ...          (tsr-6ph)",
            Self::NamespaceExport => "export * as ns          (tsr-6ph)",
            Self::OtherAlias => "other import alias      (export symbol, not a module object)",
            Self::LocalNamespace => "local namespace N {}    (same-file)",
            Self::LocalDeclaration => "local declaration       (not module-related)",
            Self::Unresolved => "name does not resolve",
            Self::NotAnIdentifier => "receiver is not a name",
            Self::Unknown => "UNKNOWN (walk failed)",
        }
    }

    /// Whether the module-object type is the thing that would answer this line.
    const fn is_module_object(self) -> bool {
        matches!(self, Self::NamespaceImport | Self::ImportEquals | Self::NamespaceExport)
    }
}

/// A bucket's lines, and the cases they came from, for concentration.
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

    /// The cases holding the most lines, named. A share alone cannot say
    /// whether a row is one pathological file or a real distribution.
    fn top_cases(&self, n: usize) -> Vec<(String, usize)> {
        let mut entries: Vec<(String, usize)> =
            self.per_case.iter().map(|(case, n)| (case.clone(), *n)).collect();
        entries.sort_by_key(|(case, n)| (std::cmp::Reverse(*n), case.clone()));
        entries.truncate(n);
        entries
    }

    /// `(cases, top-1, top-10)` as shares of `lines`.
    #[allow(clippy::cast_precision_loss)]
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.per_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let total = self.lines.max(1) as f64;
        let top1 = counts.first().copied().unwrap_or(0) as f64 / total * 100.0;
        let top10 = counts.iter().take(10).sum::<usize>() as f64 / total * 100.0;
        (counts.len(), top1, top10)
    }
}

#[derive(Default)]
struct Report {
    /// Gradient reconciliation, so this probe's denominator can be checked
    /// against the suite's before anything here is quoted.
    right: usize,
    gap: usize,
    wrong: usize,
    /// Every gap line's row prefix, for the two rows and their siblings.
    rows: BTreeMap<String, Tally>,
    /// The verbatim reason, which is the **board's** row key.
    board_rows: BTreeMap<String, Tally>,
    /// The partition. Keyed by row so the two rows can be read apart.
    buckets: BTreeMap<(&'static str, Bucket), Tally>,
    /// The terminal `gap_reason`, verbatim, for the histogram.
    terminal: BTreeMap<String, Tally>,
    /// Per bucket, what the **baseline** prints for the terminal receiver.
    spelling: BTreeMap<(Bucket, String), usize>,
    /// Chain depth: 0 is `x.foo`, 1 is `x.a.foo`.
    depth: BTreeMap<usize, usize>,
    c1_other_row: usize,
    c2_terminal_is_access: usize,
    c3_ns_without_specifier: usize,
    c3_ns_with_specifier: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.right += other.right;
        self.gap += other.gap;
        self.wrong += other.wrong;
        for (key, tally) in &other.rows {
            self.rows.entry(key.clone()).or_default().merge(tally);
        }
        for (key, tally) in &other.board_rows {
            self.board_rows.entry(key.clone()).or_default().merge(tally);
        }
        for (key, tally) in &other.buckets {
            self.buckets.entry(*key).or_default().merge(tally);
        }
        for (key, tally) in &other.terminal {
            self.terminal.entry(key.clone()).or_default().merge(tally);
        }
        for (key, n) in &other.spelling {
            *self.spelling.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.depth {
            *self.depth.entry(*key).or_default() += n;
        }
        self.c1_other_row += other.c1_other_row;
        self.c2_terminal_is_access += other.c2_terminal_is_access;
        self.c3_ns_without_specifier += other.c3_ns_without_specifier;
        self.c3_ns_with_specifier += other.c3_ns_with_specifier;
    }
}

/// The row a reason belongs to: the prefix up to the phrase, or `None` when the
/// line is not one of these rows at all.
fn row_of(reason: &str) -> Option<&'static str> {
    if !reason.contains(PHRASE) {
        return None;
    }
    if reason.starts_with(ROW_MEMBER) {
        Some("member name")
    } else if reason.starts_with(ROW_ACCESS) {
        Some("property access")
    } else {
        None
    }
}

fn check_classifier() {
    assert_eq!(row_of("member name, the receiver is a gap: Identifier"), Some("member name"));
    assert_eq!(
        row_of("property access, the receiver is a gap: CallExpression"),
        Some("property access")
    );
    assert_eq!(row_of("member name, the receiver has no such property: string"), None);
    assert_eq!(row_of("reference, symbol has no type: SymbolFlags(ALIAS)"), None);
    // A third prefix carrying the phrase is control C1's input, and there is no
    // such call site in `types_producer` today.
    assert_eq!(row_of("something else, the receiver is a gap: Identifier"), None);
    assert!(Bucket::NamespaceImport.is_module_object());
    assert!(!Bucket::OtherAlias.is_module_object(), "an export symbol is not a module object");
    assert!(!Bucket::LocalDeclaration.is_module_object());
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

#[allow(clippy::too_many_lines)]
fn measure(case: &tsr_conformance::CaseEntry) -> Option<Report> {
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));

    let mut report = Report::default();
    let name = &case.name;

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        // Every rendered line's node, so the terminal receiver can be looked up
        // in the *baseline* and its expected right-hand side read off.
        let position_of: BTreeMap<NodeId, usize> = line_ids
            .iter()
            .enumerate()
            .map(|(position, id)| (*id, position))
            .collect::<BTreeMap<_, _>>();

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
            if reason.contains(PHRASE) && row_of(&reason).is_none() {
                report.c1_other_row += 1;
            }
            let Some(row) = row_of(&reason) else {
                continue;
            };
            report.rows.entry(row.to_string()).or_default().add(name, 1);
            // The board's row key is the **whole** reason, receiver kind and
            // all, so a figure quoted off the board reconciles against this
            // histogram and not against the two-row total above. Keeping both
            // is the difference between agreeing with `tsr-6ph`'s 1,590 and
            // guessing at what it counted.
            report.board_rows.entry(reason.clone()).or_default().add(name, 1);

            // The access the row is about: the line's node when it *is* the
            // access, its parent when the line is the member name.
            let access = if nodes.kind(id) == SyntaxKind::PropertyAccessExpression {
                Some(id)
            } else {
                nodes.parent(id).filter(|&p| nodes.kind(p) == SyntaxKind::PropertyAccessExpression)
            };
            let Some(mut access) = access else {
                report.buckets.entry((row, Bucket::Unknown)).or_default().add(name, 1);
                continue;
            };

            // Descend while the *measured* reason keeps saying the receiver is
            // the gap. Never assumed: each step re-asks `gap_reason`.
            let mut depth = 0usize;
            let (terminal, terminal_reason) = loop {
                let Some(Node::PropertyAccessExpression(node)) = map.get(access) else {
                    break (None, String::new());
                };
                let Some(receiver) =
                    node.expression.as_ref().and_then(tsr_ast::Expression::node_id)
                else {
                    break (None, String::new());
                };
                let reason = types_producer::gap_reason(&mut checker, bound, nodes, map, receiver);
                if reason.contains(PHRASE)
                    && nodes.kind(receiver) == SyntaxKind::PropertyAccessExpression
                {
                    access = receiver;
                    depth += 1;
                    continue;
                }
                break (Some(receiver), reason);
            };
            *report.depth.entry(depth).or_default() += 1;
            let Some(terminal) = terminal else {
                report.buckets.entry((row, Bucket::Unknown)).or_default().add(name, 1);
                continue;
            };
            if terminal_reason.contains(PHRASE) {
                report.c2_terminal_is_access += 1;
            }
            report.terminal.entry(terminal_reason).or_default().add(name, 1);

            let bucket = classify(bound, nodes, map, terminal, &mut report);
            report.buckets.entry((row, bucket)).or_default().add(name, 1);

            // What upstream prints for the receiver itself. The spellability
            // bound: `>ns : typeof ns` is the local alias, never the module.
            if let Some(&receiver_position) = position_of.get(&terminal)
                && let Some(line) = expected_file.assertions.get(receiver_position)
                && let Some(ours_text) = our_file.get(receiver_position).map(|a| a.text.clone())
                && let Some(rhs) = line.text.strip_prefix(&format!("{ours_text} : "))
            {
                *report.spelling.entry((bucket, rhs.to_string())).or_default() += 1;
            }
        }
    }
    Some(report)
}

/// What declares the terminal receiver.
fn classify(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    terminal: NodeId,
    report: &mut Report,
) -> Bucket {
    let Some(node) = map.get(terminal) else { return Bucket::Unknown };
    let Node::Identifier(identifier) = node else { return Bucket::NotAnIdentifier };
    // `SymbolFlags::VALUE`, mirroring `expressions.rs`'s identifier arm and
    // `gap_reason`'s. A wider meaning here would resolve names the checker did
    // not, and attribute lines to a declaration it never saw.
    let symbol = identifier
        .node_id
        .and_then(|id| bound.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE));
    let Some(symbol) = symbol else { return Bucket::Unresolved };
    let kinds: BTreeSet<SyntaxKind> =
        bound.symbols().get(symbol).declarations.iter().map(|&d| nodes.kind(d)).collect();

    if kinds.contains(&SyntaxKind::NamespaceImport) {
        // Control C3: the grammar puts a `NamespaceImport` inside an
        // `ImportDeclaration`, which cannot exist without a module specifier.
        let specifier = bound
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .filter(|&&d| nodes.kind(d) == SyntaxKind::NamespaceImport)
            .any(|&d| has_specifier(nodes, map, d));
        if specifier {
            report.c3_ns_with_specifier += 1;
        } else {
            report.c3_ns_without_specifier += 1;
        }
        return Bucket::NamespaceImport;
    }
    if kinds.contains(&SyntaxKind::ImportEqualsDeclaration) {
        return Bucket::ImportEquals;
    }
    if kinds.contains(&SyntaxKind::NamespaceExport)
        || kinds.contains(&SyntaxKind::NamespaceExportDeclaration)
    {
        return Bucket::NamespaceExport;
    }
    if kinds.contains(&SyntaxKind::ImportSpecifier)
        || kinds.contains(&SyntaxKind::ImportClause)
        || kinds.contains(&SyntaxKind::ExportSpecifier)
    {
        return Bucket::OtherAlias;
    }
    if kinds.contains(&SyntaxKind::ModuleDeclaration) {
        return Bucket::LocalNamespace;
    }
    Bucket::LocalDeclaration
}

/// Walk out of an import clause to the `ImportDeclaration` and ask for its
/// module specifier. Control C3's positive test.
fn has_specifier(nodes: &tsr_ast::NodeTable, map: &tsr_ast::NodeMap<'_>, id: NodeId) -> bool {
    let mut current = Some(id);
    for _ in 0..4 {
        let Some(node) = current else { return false };
        if let Some(Node::ImportDeclaration(declaration)) = map.get(node) {
            return declaration.module_specifier.is_some();
        }
        current = nodes.parent(node);
    }
    false
}

#[allow(clippy::too_many_lines, clippy::cast_precision_loss, clippy::cast_possible_wrap)]
fn print(report: &Report) {
    let total = report.right + report.gap + report.wrong;
    println!("# receiver_gap — what actually gaps the receiver\n");
    println!(
        "gradient: {} lines = right {} ({:.2}%) + gap {} ({:.2}%) + wrong {} ({:.2}%)\n",
        total,
        report.right,
        report.right as f64 / total.max(1) as f64 * 100.0,
        report.gap,
        report.gap as f64 / total.max(1) as f64 * 100.0,
        report.wrong,
        report.wrong as f64 / total.max(1) as f64 * 100.0,
    );

    println!("## The two rows\n");
    println!("{:<18} {:>7} {:>7} {:>8} {:>8}", "row", "lines", "cases", "top-1", "top-10");
    let mut rows_total = 0usize;
    for (row, tally) in &report.rows {
        let (cases, top1, top10) = tally.concentration();
        println!("{:<18} {:>7} {:>7} {:>7.1}% {:>7.1}%", row, tally.lines, cases, top1, top10);
        let named: Vec<String> =
            tally.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
        println!("        top: {}", named.join(" | "));
        rows_total += tally.lines;
    }
    println!("{:<18} {:>7}\n", "TOTAL", rows_total);

    println!("## The board's row keys — the whole reason, receiver kind and all\n");
    let mut board: Vec<_> = report.board_rows.iter().collect();
    board.sort_by_key(|(_, tally)| std::cmp::Reverse(tally.lines));
    for (reason, tally) in board.iter().take(12) {
        let (cases, top1, top10) = tally.concentration();
        println!(
            "{:>6} {:>5}c {:>6.1}%t1 {:>6.1}%t10  {}",
            tally.lines, cases, top1, top10, reason
        );
    }
    println!();

    println!("## What gaps the receiver — the partition\n");
    println!(
        "{:<58} {:>7} {:>7} {:>7} {:>8} {:>8}",
        "bucket", "member", "access", "lines", "cases", "top-10"
    );
    let mut by_bucket: BTreeMap<Bucket, (usize, usize)> = BTreeMap::new();
    for ((row, bucket), tally) in &report.buckets {
        let entry = by_bucket.entry(*bucket).or_default();
        if *row == "member name" {
            entry.0 += tally.lines;
        } else {
            entry.1 += tally.lines;
        }
    }
    let mut partition_total = 0usize;
    let mut module_object = 0usize;
    for (bucket, (member, access)) in &by_bucket {
        let lines = member + access;
        partition_total += lines;
        if bucket.is_module_object() {
            module_object += lines;
        }
        let mut merged = Tally::default();
        for row in ["member name", "property access"] {
            if let Some(tally) = report.buckets.get(&(row, *bucket)) {
                merged.merge(tally);
            }
        }
        let (cases, _, top10) = merged.concentration();
        println!(
            "{:<58} {:>7} {:>7} {:>7} {:>7} {:>7.1}%",
            bucket.label(),
            member,
            access,
            lines,
            cases,
            top10
        );
        let named: Vec<String> =
            merged.top_cases(3).into_iter().map(|(case, n)| format!("{case} {n}")).collect();
        println!("        top: {}", named.join(" | "));
    }
    println!("{:<58} {:>23}\n", "PARTITION TOTAL", partition_total);
    println!(
        "module-object forms: {} of {} classified ({:.1}%)  —  everything else: {} ({:.1}%)\n",
        module_object,
        partition_total,
        module_object as f64 / partition_total.max(1) as f64 * 100.0,
        partition_total - module_object,
        (partition_total - module_object) as f64 / partition_total.max(1) as f64 * 100.0,
    );

    println!("## Terminal `gap_reason` — top 20 verbatim\n");
    let mut terminal: Vec<_> = report.terminal.iter().collect();
    terminal.sort_by_key(|(_, tally)| std::cmp::Reverse(tally.lines));
    for (reason, tally) in terminal.iter().take(20) {
        let (cases, _, top10) = tally.concentration();
        println!("{:>6} {:>5}c {:>6.1}%t10  {}", tally.lines, cases, top10, reason);
    }
    println!();

    println!("## Chain depth (0 = `x.foo`, 1 = `x.a.foo`)\n");
    for (depth, n) in &report.depth {
        println!("  depth {depth:>2}: {n}");
    }
    println!();

    println!("## Can we spell it? The baseline's own RHS for the terminal receiver\n");
    let mut spelling: BTreeMap<Bucket, Vec<(&String, usize)>> = BTreeMap::new();
    for ((bucket, rhs), n) in &report.spelling {
        spelling.entry(*bucket).or_default().push((rhs, *n));
    }
    for (bucket, mut entries) in spelling {
        entries.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let total: usize = entries.iter().map(|(_, n)| n).sum();
        println!("  {} — {} lines with a baseline RHS", bucket.label(), total);
        for (rhs, n) in entries.iter().take(6) {
            println!("      {n:>5}  {rhs}");
        }
    }
    println!();

    println!("## Controls\n");
    println!(
        "  C1  the phrase under a third row prefix          = {} (must be 0, by construction)",
        report.c1_other_row
    );
    println!(
        "  C2  terminal reason still says receiver-is-a-gap = {} (must be 0, by construction)",
        report.c2_terminal_is_access
    );
    println!(
        "  C3  `import * as ns` root with no specifier      = {} (must be 0, by the grammar)",
        report.c3_ns_without_specifier
    );
    println!(
        "  C3' `import * as ns` root with one               = {} (the mirror; pins the classifier)",
        report.c3_ns_with_specifier
    );
    println!(
        "  A1  rows {} == partition {}  (difference {})",
        rows_total,
        partition_total,
        rows_total as i64 - partition_total as i64
    );
}
