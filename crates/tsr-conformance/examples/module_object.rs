//! Can this port **spell** the answer to a module-object line?
//!
//! `bd tsr-6ph` sizes the module-object half of the cross-file alias seam at
//! **3,539 seam-only lines** — every gap line whose blocking leaves are all
//! cross-file aliases and whose worst leaf is a namespace-shaped form
//! (`import * as ns`, `import a = require(...)`, `export * as ns`), which
//! `examples/module_blocked.rs` buckets as `Mock::NotMocked` rather than
//! folding into its conversion count.
//!
//! `bd tsr-4jk` measured, over the same corpus, that upstream's assertion on the
//! **declaration name** of those very forms is the *local alias*, never the
//! module: `typeof cjs` 236, `typeof type` 184, `typeof cjsi` 172, `typeof mjs`
//! 156. A module symbol's name in this port is the file path with its extension
//! stripped (`bind_source_file_as_external_module`,
//! `crates/tsr-binder/src/binder.rs`), so answering those lines would print
//! `typeof /0` where upstream prints `typeof ns` — a **wrong** line where a gap
//! stands, which ADR-0038 and ADR-0039 exist to refuse.
//!
//! The two numbers have never been reconciled, and they overlap: two of
//! `tsr-4jk`'s three forms are `tsr-6ph`'s two largest. This probe answers
//! `docs/conventions.md`'s first ranking question — *"can this port spell the
//! answer?"* — over the 3,539, by splitting them on what upstream actually
//! prints:
//!
//! * **unspellable** — upstream's answer names the module object itself
//!   (`typeof <local alias>`, or a qualified name rooted at one), and this port
//!   would have to name it by its path;
//! * **spellable** — upstream's answer is anything else, or the target module
//!   has an `export = X` and so resolves through `resolveExternalModuleSymbol`
//!   (`checker.go:15556`) to an ordinary named symbol.
//!
//! The `export =` sub-case is not left as a population: it is *mocked*, exactly
//! as `module_blocked` mocks the member forms — the target file is resolved, its
//! `export=` symbol's type is asked for, and the answer is compared against
//! upstream's text. So its column is a measured conversion, not a ceiling.
//!
//! # Instrument
//!
//! Every measurement here routes through
//! `types_producer::assertions_for_case_with_ids`, which builds one program per
//! case with every bundled lib (`types_producer.rs:49`, `:725`). A probe that
//! builds its own per-file checker measures a lib-less compiler and reports
//! wrong answers as honest gaps — three instances, `bd tsr-qj4`,
//! `docs/conventions.md` "A probe that re-implements the harness is measuring a
//! different compiler". The walk, the seed classifier and the span test below
//! are `examples/module_blocked.rs`'s, reproduced rather than shared because an
//! example cannot import another example; control C1 reconciles this probe's
//! seam-only totals against that one's published figures.
//!
//! ```text
//! cargo run -p tsr-conformance --release --example module_object
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The row `bd tsr-4jk` is about: the declaration name of the import itself.
const ROW_DECL: &str =
    "declaration name, symbol has no type: SymbolFlags(ALIAS) / no value declaration";

/// Whether an alias form needs a module graph to answer.
/// `examples/module_blocked.rs:136`, verbatim.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reach {
    SameFile,
    CrossFile,
    Neither,
}

/// `examples/module_blocked.rs:156`, verbatim.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Blockers {
    cross: bool,
    same: bool,
    neither: bool,
    other: bool,
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

    fn seam_only(self) -> bool {
        self.cross && !self.same && !self.neither && !self.other && !self.unresolved
    }

    fn any(self) -> bool {
        self.cross || self.same || self.neither || self.other || self.unresolved
    }
}

/// `examples/module_blocked.rs:192`, verbatim — including the declaration
/// order, which `blockers_of` takes the minimum over to find the worst leaf.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Mock {
    NoSuchFile,
    NoSuchExport,
    NotMocked,
    StillError,
    WouldBeWrong,
    WouldConvert,
}

impl Mock {
    fn label(self) -> &'static str {
        match self {
            Self::NoSuchFile => "specifier names no file in the program",
            Self::NoSuchExport => "the file has no export of that name",
            Self::NotMocked => "namespace-shaped target — THIS PROBE'S POPULATION",
            Self::StillError => "target found, still `error`",
            Self::WouldBeWrong => "target found, typed, WRONG type",
            Self::WouldConvert => "WOULD CONVERT — exactly upstream's type",
        }
    }
}

/// What lies behind one namespace-shaped seed, once the specifier is resolved.
///
/// Ordered worst-last, so aggregating with `max` over the seeds beneath a line
/// says *"the least spellable thing this line depends on"*: one plain module
/// object under a call expression is enough to make the whole line unspellable,
/// however many `export =` targets sit beside it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum NsTarget {
    /// `export = X`, and `get_type_of_symbol(X)` is **exactly** upstream's text.
    ExportEqualsConverts,
    /// `export = X`, typed, and the text differs. Spellable, not yet right.
    ExportEqualsWrong,
    /// `export = X`, and the port still answers `error` for X. The module-object
    /// seam is necessary and not sufficient here.
    ExportEqualsStillError,
    /// No `export =`: the answer is the module object, whose only name in this
    /// port is the file path.
    PlainModuleObject,
    /// The specifier named no file this probe's resolver could place, so
    /// nothing is claimed about it either way.
    NoTargetFile,
}

impl NsTarget {
    fn label(self) -> &'static str {
        match self {
            Self::ExportEqualsConverts => "export = X, type is exactly upstream's   SPELLABLE",
            Self::ExportEqualsWrong => "export = X, typed, text differs          spellable",
            Self::ExportEqualsStillError => "export = X, still `error`                next blocker",
            Self::PlainModuleObject => "plain module object                      UNSPELLABLE",
            Self::NoTargetFile => "specifier names no file                  unknown",
        }
    }
}

/// Whether upstream's answer for a line **names the module object**.
///
/// The whole question, as a predicate on the baseline's right-hand side. A
/// module object's only name in this port is its path, so an answer that names
/// one cannot be printed correctly whatever the checker resolves.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Spell {
    /// `typeof ns` — the answer *is* the module object.
    IsModuleObject,
    /// `typeof ns.inner` — rooted at one.
    RootedAtModuleObject,
    /// `import("./m").Widget` — upstream's node builder printing a symbol that
    /// has **no accessible name** at the reference site. A second rendering
    /// capability this port does not have and that no module-object type
    /// supplies; folding it into "ordinary" would count the `privacy*` family
    /// as available work.
    ImportTypeForm,
    /// Anything else: `number`, `typeof C`, `(x: string) => void`. The names in
    /// such an answer are ordinary symbols carrying their own names.
    Ordinary,
}

impl Spell {
    fn label(self) -> &'static str {
        match self {
            Self::IsModuleObject => "answer IS the module object      `typeof ns`",
            Self::RootedAtModuleObject => "answer is ROOTED at one          `typeof ns.x`",
            Self::ImportTypeForm => "answer is an import type         `import(\"m\").W`",
            Self::Ordinary => "ordinary answer                  spellable",
        }
    }
}

/// Classify upstream's answer text against the namespace aliases bound in the
/// case.
///
/// `aliases` is the set of local names bound by a namespace-shaped cross-file
/// form anywhere in the case — the exact set of names whose symbol this port
/// would have to render as a module object.
fn spell_of(want: &str, aliases: &BTreeSet<String>) -> Spell {
    if want.contains("import(\"") {
        return Spell::ImportTypeForm;
    }
    let Some(rest) = want.strip_prefix("typeof ") else { return Spell::Ordinary };
    for alias in aliases {
        if rest == alias {
            return Spell::IsModuleObject;
        }
        if rest.starts_with(alias.as_str()) && rest.as_bytes().get(alias.len()) == Some(&b'.') {
            return Spell::RootedAtModuleObject;
        }
    }
    Spell::Ordinary
}

/// One namespace-shaped seed, kept for the seed-level table and control C2.
struct NsSeed {
    form: String,
    /// The local name the import binds — what upstream prints.
    alias: String,
    /// The target module symbol's name in this port — what this port would
    /// print. Control C2 is that these two are never equal.
    module_name: Option<String>,
    target: NsTarget,
    want: String,
}

#[derive(Default)]
struct CaseReport {
    name: String,
    /// Every seam-only gap line, by the worst mock beneath it. Control C1.
    seam_worst: BTreeMap<Mock, usize>,
    /// The population: seam-only lines whose worst leaf is namespace-shaped,
    /// crossed by what upstream prints and what lies behind the target.
    split: BTreeMap<(Spell, NsTarget), usize>,
    /// Upstream's answer text for the unspellable half, and for the spellable.
    unspellable_wants: BTreeMap<String, usize>,
    spellable_wants: BTreeMap<String, usize>,
    /// Per-case concentration of the spellable half.
    spellable_lines: usize,
    unspellable_lines: usize,
    /// Spellable only because the target has an `export =`. Reported apart,
    /// because its conversion is a separate measured question.
    via_export_equals: usize,
    seeds: Vec<NsSeed>,
    /// Control C2: namespace seeds whose module symbol name equals the alias.
    name_collisions: usize,
    /// Control C3: `ROW_DECL` seeds on a plain module object whose answer is
    /// NOT `typeof <the alias>`.
    decl_plain: usize,
    decl_plain_not_alias: usize,
    /// The C3 exceptions, printed rather than counted: a control that only
    /// prints a number cannot be acted on when it is non-zero.
    decl_exceptions: Vec<(String, String)>,
}

/// `rank_board::row_key` (`examples/rank_board.rs:145`), verbatim.
fn row_key(reason: &str) -> String {
    for cut in ["has no such property: ", "unresolved: "] {
        if let Some(at) = reason.find(cut) {
            return reason[..at + cut.len() - 2].to_string();
        }
    }
    reason.to_string()
}

fn is_alias_reason(reason: &str) -> bool {
    reason.contains("SymbolFlags(ALIAS") && reason.contains("/ no value declaration")
}

/// The properties the classifiers rest on, checked on every run rather than in
/// `#[cfg(test)]`: Cargo does not run tests inside an example unless the
/// manifest declares `test = true`, and the manifest is shared.
fn check_classifier() {
    assert!(is_alias_reason(ROW_DECL), "the named row is a seed");
    assert_eq!(row_key(ROW_DECL), ROW_DECL, "row_key is the identity on the named row");

    let aliases: BTreeSet<String> = ["ns", "React"].iter().map(ToString::to_string).collect();
    // The three answers the split turns on, and the two near misses that must
    // NOT be classified as naming a module object.
    assert_eq!(spell_of("typeof ns", &aliases), Spell::IsModuleObject);
    assert_eq!(spell_of("typeof ns.inner", &aliases), Spell::RootedAtModuleObject);
    assert_eq!(spell_of("typeof C", &aliases), Spell::Ordinary, "an ordinary class");
    assert_eq!(
        spell_of("typeof nsOther", &aliases),
        Spell::Ordinary,
        "a prefix of an alias is not the alias"
    );
    assert_eq!(spell_of("ns", &aliases), Spell::Ordinary, "an instance type, not the object");
    assert_eq!(
        spell_of("() => import(\"./m\").Widget1", &aliases),
        Spell::ImportTypeForm,
        "an inaccessible symbol printed as an import type is a second naming capability"
    );

    // Worst-leaf aggregation: `max` must pick the unspellable one.
    assert_eq!(
        NsTarget::ExportEqualsConverts.max(NsTarget::PlainModuleObject),
        NsTarget::PlainModuleObject,
        "one plain module object under a line makes the line unspellable"
    );
    assert!(Blockers { cross: true, ..Blockers::default() }.seam_only());
    assert!(
        !Blockers { cross: true, other: true, ..Blockers::default() }.seam_only(),
        "one non-alias blocker takes a line out of the seam population"
    );
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
    let node_map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::new(bound, nodes, node_map);
    // A second checker for the mock, so asking for a target's type cannot
    // perturb the memo the reason strings are read out of.
    let mut mock_checker = tsr_checker::Checker::new(bound, nodes, node_map);
    let error = mock_checker.intrinsics().error;

    let mut report = CaseReport { name: case.name.clone(), ..CaseReport::default() };

    for (index, expected_file) in expected.iter().enumerate() {
        let our_file = ours.get(index);
        let our_ids = ids.get(index);

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
            let Some(got) = our_file.and_then(|file| file.get(position)) else { continue };
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if want_type == got.type_string || got.type_string != "error" {
                continue;
            }
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

        // Pass two: classify the seeds. The namespace-shaped ones get their
        // target resolved and, where it is an `export =`, its type asked for.
        let mut seed_of: HashMap<usize, Reach> = HashMap::new();
        let mut mock_of: HashMap<usize, Mock> = HashMap::new();
        let mut ns_of: HashMap<usize, NsTarget> = HashMap::new();
        let mut aliases: BTreeSet<String> = BTreeSet::new();
        let mut pending: Vec<(usize, String, String, String, Option<String>, NsTarget)> =
            Vec::new();
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
            let want = wants.get(&position).map_or("", String::as_str);
            let mock = match reach {
                Reach::CrossFile => Some(mock_target(
                    &program,
                    &mut mock_checker,
                    bound,
                    nodes,
                    node_map,
                    declaration,
                    specifier.as_deref(),
                    want,
                    error,
                )),
                _ => None,
            };
            seed_of.insert(position, reach);
            if let Some(mock) = mock {
                mock_of.insert(position, mock);
            }
            if mock == Some(Mock::NotMocked) {
                let alias = bound.symbols().get(symbol).name.to_string();
                aliases.insert(alias.clone());
                let (target, module_name) = mock_namespace(
                    &program,
                    &mut mock_checker,
                    bound,
                    nodes,
                    declaration,
                    specifier.as_deref(),
                    want,
                    error,
                );
                ns_of.insert(position, target);
                pending.push((position, form, row_key(reason), alias, module_name, target));
            }
        }

        for (position, form, row, alias, module_name, target) in pending {
            let want = wants.get(&position).cloned().unwrap_or_default();
            if module_name.as_deref() == Some(alias.as_str()) {
                report.name_collisions += 1;
            }
            if row == ROW_DECL && target == NsTarget::PlainModuleObject {
                report.decl_plain += 1;
                if want != format!("typeof {alias}") {
                    report.decl_plain_not_alias += 1;
                    report.decl_exceptions.push((alias.clone(), want.clone()));
                }
            }
            report.seeds.push(NsSeed { form, alias, module_name, target, want });
        }

        // Pass three: walk every gap line down to its blocking leaves.
        let mut memo: HashMap<usize, (Blockers, Option<Mock>, Option<NsTarget>)> = HashMap::new();
        for &position in reasons.keys() {
            let mut stack = Vec::new();
            let (blockers, worst, ns) = blockers_of(
                position,
                &reasons,
                &seed_of,
                &mock_of,
                &ns_of,
                &position_of,
                file_lines,
                line_ids,
                nodes,
                node_map,
                bound,
                &mut memo,
                &mut stack,
            );
            if !blockers.seam_only() {
                continue;
            }
            let Some(worst) = worst else { continue };
            *report.seam_worst.entry(worst).or_default() += 1;
            if worst != Mock::NotMocked {
                continue;
            }
            let want = wants.get(&position).cloned().unwrap_or_default();
            let spell = spell_of(&want, &aliases);
            let ns = ns.unwrap_or(NsTarget::NoTargetFile);
            *report.split.entry((spell, ns)).or_default() += 1;
            // A line is spellable if upstream's answer never names a module
            // object, OR every module object beneath it resolves through an
            // `export =` to an ordinary named symbol.
            let via_export_equals = matches!(
                ns,
                NsTarget::ExportEqualsConverts
                    | NsTarget::ExportEqualsWrong
                    | NsTarget::ExportEqualsStillError
            );
            let spellable = spell == Spell::Ordinary || via_export_equals;
            if spellable && via_export_equals && spell != Spell::Ordinary {
                report.via_export_equals += 1;
            }
            if spellable {
                report.spellable_lines += 1;
                *report.spellable_wants.entry(want).or_default() += 1;
            } else {
                report.unspellable_lines += 1;
                *report.unspellable_wants.entry(want).or_default() += 1;
            }
        }
    }
    Some(report)
}

/// `examples/module_blocked.rs:580`, verbatim.
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

/// `examples/module_blocked.rs:596`, verbatim.
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
                ("export { q }            (same file)".to_string(), Reach::SameFile)
            }
        }
        Some(Node::ExportAssignment(node)) => match node.expression {
            Some(tsr_ast::Expression::Identifier(_)) if node.is_export_equals => {
                ("export = x              (same file)".to_string(), Reach::SameFile)
            }
            Some(tsr_ast::Expression::Identifier(_)) => {
                ("export default x        (same file)".to_string(), Reach::SameFile)
            }
            _ => ("export default <expr>   (same file)".to_string(), Reach::SameFile),
        },
        Some(Node::NamespaceExportDeclaration(_)) => {
            ("export as namespace N   (neither half)".to_string(), Reach::Neither)
        }
        _ => (format!("UNCLASSIFIED FORM: {:?}", nodes.kind(declaration)), Reach::Neither),
    }
}

/// `examples/module_blocked.rs:652`, verbatim.
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

/// `examples/module_blocked.rs:669`, verbatim.
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

/// `examples/module_blocked.rs:720`, verbatim — the member-form mock, kept so
/// that this probe's `Mock::NotMocked` population is the *same* population
/// `module_blocked` publishes as 3,539.
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

/// **Probe past the blocker, for the namespace-shaped forms.**
///
/// Resolves the specifier to a file and asks what a module object would have to
/// be named here, and whether the module has an `export =` that
/// `resolveExternalModuleSymbol` (`checker.go:15556`) would return instead. In
/// the `export =` case the target's type is asked for and compared against
/// upstream's text, so that column is a *conversion* and not a population.
///
/// Returns the target's kind and the module symbol's own name — the string this
/// port would have to print for a plain module object, and the subject of
/// control C2.
#[allow(clippy::too_many_arguments)]
fn mock_namespace(
    program: &tsr_compiler::Program<'_>,
    checker: &mut tsr_checker::Checker<'_, '_>,
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    declaration: NodeId,
    specifier: Option<&str>,
    want: &str,
    error: tsr_checker::TypeId,
) -> (NsTarget, Option<String>) {
    let Some(specifier) = specifier else { return (NsTarget::NoTargetFile, None) };
    let Some(file) = resolve_specifier(program, nodes, declaration, specifier) else {
        return (NsTarget::NoTargetFile, None);
    };
    let Some(root) = file.source_file().node_id else { return (NsTarget::NoTargetFile, None) };
    let Some(module) = bound.symbol_of(root) else { return (NsTarget::NoTargetFile, None) };
    let module_name = bound.symbols().get(module).name.to_string();
    // `bind_source_file_as_external_module` names the module symbol after the
    // file. `INTERNAL_EXPORT_EQUALS` (`crates/tsr-binder/src/binder.rs:3527`)
    // is the key an `export = X` binds under.
    let Some(&target) = bound.symbols().get(module).exports.get("export=") else {
        return (NsTarget::PlainModuleObject, Some(module_name));
    };
    let answer = checker.get_type_of_symbol(target);
    let kind = if answer == error {
        NsTarget::ExportEqualsStillError
    } else if checker.type_to_string(answer) == want {
        NsTarget::ExportEqualsConverts
    } else {
        NsTarget::ExportEqualsWrong
    };
    (kind, Some(module_name))
}

fn module_export_name(name: tsr_ast::ModuleExportName<'_>) -> String {
    match name {
        tsr_ast::ModuleExportName::Identifier(identifier) => identifier.text.to_string(),
        tsr_ast::ModuleExportName::StringLiteral(string) => string.text.to_string(),
    }
}

/// `examples/module_blocked.rs:776`, verbatim.
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
    let trimmed: Vec<String> = [".js", ".mjs", ".cjs", ".jsx"]
        .iter()
        .filter_map(|extension| joined.strip_suffix(extension).map(ToString::to_string))
        .collect();
    let mut bases: Vec<&str> = vec![joined.as_str(), specifier];
    bases.extend(trimmed.iter().map(String::as_str));
    for base in bases {
        for extension in extensions {
            if let Some(file) = program.source_file(&format!("{base}{extension}")) {
                if file.file_name() != containing.file_name() {
                    return Some(file);
                }
            }
        }
    }
    let _ = nodes;
    None
}

/// `examples/module_blocked.rs:831`, with a third channel: the worst
/// `NsTarget` beneath the line, aggregated by `max`.
#[allow(clippy::too_many_arguments)]
fn blockers_of(
    position: usize,
    reasons: &HashMap<usize, String>,
    seed_of: &HashMap<usize, Reach>,
    mock_of: &HashMap<usize, Mock>,
    ns_of: &HashMap<usize, NsTarget>,
    position_of: &HashMap<NodeId, usize>,
    lines: &[types_producer::Assertion],
    line_ids: &[NodeId],
    nodes: &tsr_ast::NodeTable,
    node_map: &tsr_ast::NodeMap<'_>,
    bound: &tsr_binder::BindResult<'_>,
    memo: &mut HashMap<usize, (Blockers, Option<Mock>, Option<NsTarget>)>,
    stack: &mut Vec<usize>,
) -> (Blockers, Option<Mock>, Option<NsTarget>) {
    if let Some(&known) = memo.get(&position) {
        return known;
    }
    if stack.contains(&position) {
        return (Blockers { unresolved: true, ..Blockers::default() }, None, None);
    }
    stack.push(position);
    let answer = (|| {
        if let Some(&reach) = seed_of.get(&position) {
            let mock = mock_of.get(&position).copied();
            let ns = ns_of.get(&position).copied();
            return match reach {
                Reach::CrossFile => (Blockers { cross: true, ..Blockers::default() }, mock, ns),
                Reach::SameFile => (Blockers { same: true, ..Blockers::default() }, None, None),
                Reach::Neither => (Blockers { neither: true, ..Blockers::default() }, None, None),
            };
        }
        let Some(reason) = reasons.get(&position) else {
            return (Blockers { unresolved: true, ..Blockers::default() }, None, None);
        };
        if let Some(dependency) =
            named_dependency(position, reason, line_ids, nodes, node_map, bound)
        {
            return match position_of.get(&dependency) {
                Some(&at) if at != position => blockers_of(
                    at,
                    reasons,
                    seed_of,
                    mock_of,
                    ns_of,
                    position_of,
                    lines,
                    line_ids,
                    nodes,
                    node_map,
                    bound,
                    memo,
                    stack,
                ),
                _ => (Blockers { unresolved: true, ..Blockers::default() }, None, None),
            };
        }
        let mut answer = Blockers::default();
        let mut worst: Option<Mock> = None;
        let mut worst_ns: Option<NsTarget> = None;
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
                let (below, below_worst, below_ns) = blockers_of(
                    index,
                    reasons,
                    seed_of,
                    mock_of,
                    ns_of,
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
                worst_ns = match (worst_ns, below_ns) {
                    (Some(left), Some(right)) => Some(left.max(right)),
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (None, None) => None,
                };
            }
            index += 1;
        }
        if answer.any() {
            (answer, worst, worst_ns)
        } else {
            (Blockers { other: true, ..Blockers::default() }, None, None)
        }
    })();
    stack.pop();
    memo.insert(position, answer);
    answer
}

/// `examples/module_blocked.rs:942`, verbatim.
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
    if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let head: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

#[allow(clippy::too_many_lines)]
fn report(reports: &[CaseReport]) {
    let mut seam_worst: BTreeMap<Mock, usize> = BTreeMap::new();
    let mut split: BTreeMap<(Spell, NsTarget), usize> = BTreeMap::new();
    let mut unspellable_wants: BTreeMap<String, usize> = BTreeMap::new();
    let mut spellable_wants: BTreeMap<String, usize> = BTreeMap::new();
    let mut seeds_by_form: BTreeMap<String, BTreeMap<NsTarget, usize>> = BTreeMap::new();
    let mut example_names: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    let (mut spellable, mut unspellable) = (0usize, 0usize);
    let (mut collisions, mut decl_plain, mut decl_plain_not_alias) = (0usize, 0usize, 0usize);
    let mut via_export_equals = 0usize;
    let mut decl_exceptions: Vec<(String, String, String)> = Vec::new();
    let mut spellable_by_case: BTreeMap<String, usize> = BTreeMap::new();

    for case in reports {
        for (&mock, &count) in &case.seam_worst {
            *seam_worst.entry(mock).or_default() += count;
        }
        for (&key, &count) in &case.split {
            *split.entry(key).or_default() += count;
        }
        for (want, &count) in &case.unspellable_wants {
            *unspellable_wants.entry(want.clone()).or_default() += count;
        }
        for (want, &count) in &case.spellable_wants {
            *spellable_wants.entry(want.clone()).or_default() += count;
        }
        spellable += case.spellable_lines;
        unspellable += case.unspellable_lines;
        if case.spellable_lines > 0 {
            *spellable_by_case.entry(case.name.clone()).or_default() += case.spellable_lines;
        }
        via_export_equals += case.via_export_equals;
        for (alias, want) in &case.decl_exceptions {
            decl_exceptions.push((case.name.clone(), alias.clone(), want.clone()));
        }
        collisions += case.name_collisions;
        decl_plain += case.decl_plain;
        decl_plain_not_alias += case.decl_plain_not_alias;
        for seed in &case.seeds {
            *seeds_by_form.entry(seed.form.clone()).or_default().entry(seed.target).or_default() +=
                1;
            if seed.target == NsTarget::PlainModuleObject {
                example_names.entry(case.name.clone()).or_insert((
                    seed.alias.clone(),
                    seed.module_name.clone().unwrap_or_default(),
                    seed.want.clone(),
                ));
            }
        }
    }

    let population: usize = seam_worst.values().sum();
    println!("=== CONTROL C1 — seam-only gap lines by the worst leaf beneath them ===");
    println!("Reconciles against examples/module_blocked.rs, which publishes 3,539 for");
    println!("`namespace-shaped target, not mocked`. A different number here means the two");
    println!("probes are not walking the same population and nothing below is comparable.\n");
    println!("{:<48}{:>9}{:>9}", "worst leaf", "lines", "share");
    for (&mock, &count) in &seam_worst {
        println!("{:<48}{count:>9}{:>8.1}%", mock.label(), pct(count, population));
    }
    println!("{:<48}{population:>9}", "TOTAL seam-only");

    let target: usize = seam_worst.get(&Mock::NotMocked).copied().unwrap_or_default();
    println!("\n=== THE 3,539: can this port spell the answer? ===");
    println!("Population {target}. A line is SPELLABLE when upstream's answer never names a");
    println!("module object, or when every module object beneath it resolves through an");
    println!("`export = X` (checker.go:15556) to an ordinary named symbol.\n");
    println!(
        "{:<34}{:<44}{:>9}{:>9}",
        "upstream's answer", "what lies behind the target", "lines", "share"
    );
    for (&(spell, ns), &count) in &split {
        println!("{:<34}{:<44}{count:>9}{:>8.1}%", spell.label(), ns.label(), pct(count, target));
    }
    println!("\n{:<34}{spellable:>9}{:>8.1}%", "SPELLABLE", pct(spellable, target));
    println!(
        "{:<34}{via_export_equals:>9}{:>8.1}%   (of the spellable: names a module object, but an `export =` renames it)",
        "  of which, only via `export =`",
        pct(via_export_equals, target)
    );
    println!("{:<34}{unspellable:>9}{:>8.1}%", "UNSPELLABLE", pct(unspellable, target));

    println!("\n=== CONTROL C2 (pinned by construction) — the name this port would print ===");
    println!("A module symbol's name is the file path with its extension stripped, so it can");
    println!("never be the local alias upstream prints. The expected value is 0, and it is");
    println!("fixed by the binder before any code here runs. Non-zero would mean the");
    println!("unspellability claim is wrong at its root.");
    println!("  namespace seeds whose module symbol name == the local alias: {collisions}");

    println!("\n=== CONTROL C3 — bd tsr-4jk's finding, re-measured on this population ===");
    println!("tsr-4jk measured that upstream's answer on the declaration name of a");
    println!("namespace-shaped import is ALWAYS the local alias. Predicted before running:");
    println!("the second row is ~0 over plain-module targets.");
    println!("  declaration-name seeds, plain module object:              {decl_plain}");
    println!(
        "  of those, upstream's answer is NOT `typeof <alias>`:       {decl_plain_not_alias} ({:.1}%)",
        pct(decl_plain_not_alias, decl_plain)
    );

    println!("\n=== CONTROL C3, the exceptions (all of them) ===");
    decl_exceptions.sort();
    for (case, alias, want) in &decl_exceptions {
        println!("  {:<56} alias `{alias}` -> upstream wants `{want}`", truncate(case, 54));
    }

    println!("\n=== namespace-shaped seeds, by form and target ===");
    println!("{:<40}{:<44}{:>9}", "form", "target", "seeds");
    for (form, targets) in &seeds_by_form {
        for (&ns, &count) in targets {
            println!("{form:<40}{:<44}{count:>9}", ns.label());
        }
    }

    println!("\n=== what upstream prints, UNSPELLABLE half (top 25) ===");
    let mut ranked: Vec<(&String, &usize)> = unspellable_wants.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (want, count) in ranked.iter().take(25) {
        println!("{count:>9}  {}", truncate(want, 90));
    }

    println!("\n=== what upstream prints, SPELLABLE half (top 25) ===");
    let mut ranked: Vec<(&String, &usize)> = spellable_wants.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (want, count) in ranked.iter().take(25) {
        println!("{count:>9}  {}", truncate(want, 90));
    }

    println!("\n=== where the spellable half is (top 15 cases) ===");
    let mut ranked: Vec<(&String, &usize)> = spellable_by_case.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (case, count) in ranked.iter().take(15) {
        println!("{count:>9}  {}", truncate(case, 90));
    }

    println!("\n=== alias vs module name, one example per case (top 15) ===");
    for (case, (alias, module, want)) in example_names.iter().take(15) {
        println!(
            "{:<52} upstream `typeof {alias}` — this port would name the module `{module}` (want: {})",
            truncate(case, 50),
            truncate(want, 30)
        );
    }
}
