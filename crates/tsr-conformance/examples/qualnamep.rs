//! **Design P, re-sized against the compiler design W left behind.**
//!
//! ```text
//! cargo run --release -p tsr-conformance --example qualnamep
//! ```
//!
//! # Why this is a new instrument and not a column on `qualname.rs`
//!
//! `qualname.rs` prices the **resolution** half of namespace-qualified naming:
//! designs W and R both fire on a **gap** line — a qualified type reference this
//! port answers `errorType` for — and turn it into a printed type. Their target
//! population is therefore a `depend.rs` gap bucket, and their CONVERTS column
//! counts gap→right.
//!
//! **Design P cannot convert a gap line at all.** P is upstream's
//! `getSymbolChain` (`internal/checker/nodebuilderimpl.go:1087`) plus
//! `needsQualification` (`internal/checker/symbolaccessibility.go:688`): it
//! changes the *text* a type prints under when that type's symbol is declared
//! inside a namespace and the symbol's own name does not resolve to it at the
//! reference site. A line whose answer is `error` has no symbol to qualify. So
//! P's conversions are **wrong→right**, its losses are **right→wrong**, and its
//! third bucket is wrong→a *different* wrong, which converts nothing and costs
//! nothing.
//!
//! That is a different arithmetic from W's and it needs its own columns:
//!
//! | column | what it counts |
//! |---|---|
//! | **CONVERTS** | wrong today; P's forecast text **is** the baseline's |
//! | **CHURN** | wrong today; P changes the text and it is still not the baseline's |
//! | **AT RISK** | **right today**; P changes the text, so the line is lost |
//! | NO-OP | P's forecast equals what is printed now |
//! | GAP | `error` today — P cannot reach it. Expected to contribute nothing |
//!
//! For W the at-risk column was a side condition that read zero. **For P the
//! at-risk column is one of the two terms of the trade**, because P's gains and
//! P's losses come out of the same predicate. Cycle 20b measured that trade at
//! 525 gained against 3,202 lost.
//!
//! # Every P number on record is pre-W and none of them is carried
//!
//! - the conversion column 525/614 is from **cycle 20b**, many builds ago;
//! - the at-risk column — 36 strict, 4,801 loose — was measured by `qualname.rs`
//!   at `22a5a51`, **before design W landed at `8e28971` (+3,590)**;
//! - `checker-notes-qualname.md` §9.3 records that W's conversions **created** P
//!   population: 13 lines of that build's residual are `Foo.B<Foo.A>` wanted
//!   against `B<Foo.A>` printed — the missing chain on the *outer* name, newly
//!   visible because the inner argument resolved.
//!
//! So all three columns are re-taken here in one pass, on a compiler that has W.
//!
//! # The model of `getSymbolChain`, and where it deliberately stops short
//!
//! [`symbol_chain`] is the reduction of `getSymbolChain`
//! (`nodebuilderimpl.go:1087`) that a build would actually write:
//!
//! 1. If `needsQualification` says no (`nodebuilderimpl.go:1094`), the chain is
//!    the symbol alone. Modelled by [`needs_qualification`], which is
//!    `resolve_name` at the site — upstream's `someSymbolTableInScope` walk
//!    answering *no qualifier* the moment a table in scope holds the symbol
//!    itself (`symbolaccessibility.go:702`).
//! 2. Otherwise go up to the container and recurse with
//!    `getQualifiedLeftMeaning` — `SymbolFlagsNamespace`
//!    (`nodebuilderimpl.go:1111`).
//!
//! The container is `getContainersOfSymbol` (`symbolaccessibility.go:280`),
//! whose **first and normal** answer is `getParentOfSymbol`
//! (`checker.go:14365`) — i.e. `Symbol.Parent`, which the binder sets only for a
//! symbol that lives in another symbol's table. This probe uses
//! `Symbol::parent` for exactly that reason, and reports the **AST-walk**
//! definition `qualname.rs` used (`namespace_container`, a walk to the nearest
//! enclosing `ModuleDeclaration`) as a separate, looser figure — CP6. The two
//! differ precisely on a **non-exported** local of a namespace, which upstream
//! gives no container and therefore never qualifies.
//!
//! Two arms are **declined rather than approximated**, each as its own bucket:
//!
//! - **a container that is an external module** (a source file symbol).
//!   Upstream does not print a dotted name for it; it calls
//!   `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:1104`) and prints an
//!   import specifier. That is `bd tsr-4jk`'s family and it is the shape
//!   `checker-notes-novaldecl.md` refused on naming grounds.
//! - **the alternative containers** — `getWithAlternativeContainers`
//!   (`symbolaccessibility.go:117`), re-export and `export =` routes. Modelling
//!   them can only *add* chains, so declining them makes both the CONVERTS and
//!   the AT-RISK column a floor rather than a forecast, in the same direction.
//!
//! # Controls, values fixed before the run
//!
//! - **CP1, pinned by upstream. Expect 0.** No forecast may qualify a **type
//!   parameter**: `lookupSymbolChainWorker` returns the bare symbol for one by
//!   construction (`nodebuilderimpl.go:1069`, `isTypeParameter`). A non-zero
//!   reading says the gate is not the gate upstream has.
//! - **CP2, pinned by upstream. Expect 0.** A symbol for which
//!   `needsQualification` is false must produce a forecast identical to what is
//!   printed now — that is the whole content of the test at
//!   `nodebuilderimpl.go:1094`. A non-zero reading says the chain is being built
//!   past its own stop condition, which is the defect
//!   `checker-notes-nameres.md` §51 attributed the 3,202 lost lines to.
//! - **CP3, arithmetic.** The five buckets sum to the aligned line count.
//! - **CP4, pinned by the PREVIOUS BUILD's recorded residual. Expect ≥ 13.**
//!   `checker-notes-qualname.md` §9.3 names 13 residual lines as *"the missing
//!   symbol chain on the OUTER name, newly visible because the inner argument
//!   resolved"*, in `declFileGenericType`, `genericClassesInModule`,
//!   `variableDeclaratorResolved…`, `TwoInternalModules…`,
//!   `ramdaToolsNoInfinite` and `typeNamedUndefined1,2`. Those lines are wrong
//!   today and P is the named owner. **If P converts none of them this
//!   instrument is not modelling the mechanism §9.3 saw**, and every other
//!   number it prints is unreadable. This is the control that makes the run
//!   post-W rather than nominally post-W.
//!
//!   *Recorded because it was got wrong once:* the first run read **12** and the
//!   defect was in this list, not in the mechanism — §9.3 truncates one case to
//!   `variableDeclaratorResolved…` and it was expanded here to
//!   `…ModuleName`, which is not a corpus case. The corpus case is
//!   `variableDeclaratorResolvedDuringContextualTyping` and it converts **9**.
//!   The registered value is not restated; only the subject list is corrected.
//! - **CP5, the descendant of `qualname.rs`'s C5. Expect 0.**
//!   `conformance/parserRealSource10` lost **481** lines to the reverted design
//!   because `resolve_name` could not see a namespace's exports
//!   (`checker-notes-nameres.md` §51); the arm landed at `3b7fa44`. Expect 0
//!   at-risk lines in that case. Non-zero means the stated cause is back.
//! - **CP6, positive control and cross-check. Must be > 0.** The at-risk
//!   population under `qualname.rs`'s looser AST-walk container definition. It
//!   read **36** pre-W and can only have grown, since W added right lines and
//!   removed none. A reading below 36 says the two runs are not measuring the
//!   same corpus.
//! - **CP7, positive control that this tree HAS design W. Must be > 0.** Lines
//!   that are **right today** and whose printed answer is a dotted name — W's
//!   own output is the bulk of it. A zero here means the working tree predates
//!   `8e28971` and every column below is the pre-W measurement again.
//!
//! # What this probe does not model
//!
//! - **The loose bound rewrites tokens, not types.** Strict qualifies only a
//!   line whose whole printed answer *is* a symbol's name — the gate the
//!   reverted build used and which `checker-notes-nameres.md` §49 records as
//!   load-bearing (*"without that gate it climbs from an anonymous `__function`
//!   symbol to no parent and gaps every function and object type in the
//!   corpus"*). Loose additionally rewrites identifier tokens inside signature
//!   and type-literal renderings, which is an upper bound on a **fully**
//!   qualifying node builder rather than on the design priced here.
//! - **Non-textual consequences.** A qualified name is still the same type, so
//!   unlike W this mechanism has no downstream unlock at all — which is the one
//!   respect in which P is easier to price than W was.

use std::collections::{BTreeMap, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_checker::types::TypeData;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The case `checker-notes-nameres.md` §51 names as the reverted design's
/// largest single loss — **−481 lines**. CP5's subject.
const CP5_CASE: &str = "conformance/parserRealSource10";

/// The cases `checker-notes-qualname.md` §9.3 attributes to design P in the W
/// build's residual — *"the missing symbol chain on the OUTER name, newly
/// visible because the inner argument resolved"*, 13 lines. CP4's subjects.
const CP4_CASES: &[&str] = &[
    "declFileGenericType",
    "genericClassesInModule",
    // §9.3 truncates this one to `variableDeclaratorResolved…`; the corpus case
    // is `variableDeclaratorResolvedDuringContextualTyping`.
    "variableDeclaratorResolvedDuringContextualTyping",
    "TwoInternalModulesThatMergeEachWithExportedAndNonExportedInterfacesOfTheSameName",
    "ramdaToolsNoInfinite",
    "typeNamedUndefined1",
    "typeNamedUndefined2",
];

/// Chain depth cap. Upstream has none; a cycle in `Symbol.Parent` would be a
/// binder defect, and the guard exists so that one does not hang the corpus.
const MAX_CHAIN: usize = 8;

// ---------------------------------------------------------------------------
// The probe's model of the two upstream functions.
// ---------------------------------------------------------------------------

/// `needsQualification` (`symbolaccessibility.go:688`), reduced to the test the
/// reverted build used and `qualname.rs` measured with: does the symbol's own
/// name, resolved from the reference site, come back as that same symbol?
///
/// Upstream walks every symbol table in scope and answers *no qualification
/// needed* the moment one holds the symbol itself (`symbolaccessibility.go:702`,
/// `if symbolFromSymbolTable == symbol { return true }`). `resolve_name` walks
/// the same tables in the same order, so it agrees on the first hit and differs
/// only on the later tables upstream keeps scanning — which can only make this
/// **over**-report qualification, i.e. over-report both columns together.
fn needs_qualification<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
    name: &str,
    site: NodeId,
    meaning: SymbolFlags,
) -> bool {
    match binder.resolve_name(nodes, map, site, name, meaning) {
        Some(found) => binder.merged_symbol(found) != binder.merged_symbol(symbol),
        None => true,
    }
}

/// Is this symbol an external module — a source file, or an ambient
/// `declare module "x"`?
///
/// Upstream prints a chain through one of these with
/// `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:1104`), an import
/// specifier and not a dotted name. Declined here, as its own bucket.
fn is_external_module<'a>(
    binder: &tsr_binder::BindResult<'a>,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> bool {
    binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
        match map.get(declaration) {
            Some(Node::SourceFile(_)) => true,
            Some(Node::ModuleDeclaration(module)) => module
                .name
                .is_some_and(|name| matches!(name, tsr_ast::ModuleName::StringLiteral(_))),
            _ => false,
        }
    })
}

/// Why a chain could not be built.
#[derive(Clone, PartialEq, Eq)]
enum Decline {
    /// `needsQualification` said no — the bare name is right. Not a shortfall.
    NoQualifierNeeded,
    /// `getContainersOfSymbol` has nothing: `Symbol.Parent` is unset, which is
    /// what upstream records for a **local** of a namespace.
    NoContainer,
    /// The container is an external module — `getSpecifierForModuleSymbol`.
    ///
    /// `qualifier` carries the forecast prefix the **container-qualifier
    /// design** would print, accumulated on unwind, tagged with which arm
    /// produced it:
    ///
    /// - `"alias"` — a unique in-scope alias names the container
    ///   (`getAccessibleSymbolChain`'s alias arm; the machinery is the shipped
    ///   `Checker::module_name_at`), qualifier `m4.`;
    /// - `"ambient-import"` — no alias, but the container is an ambient
    ///   `declare module "x"`, whose specifier is exact and free
    ///   (`nodebuilderimpl.go:1260`), qualifier `import("x").`.
    ///
    /// `None` — a file module with no unique alias: only the
    /// `modulespecifiers` package could spell it, and the design declines.
    ExternalModule { qualifier: Option<(&'static str, String)> },
    /// An in-scope alias resolves to the **symbol itself** —
    /// `trySymbolTable`'s direct arm (`symbolaccessibility.go:535`): the bare
    /// name is accessible, so no qualifier may fire. The guard that owns the
    /// ambient-import arm's would-be AT-RISK lines (`typeof Observable` right
    /// today via `import { Observable } from "observable"`).
    SymbolAliasedInScope,
    /// The container is not a namespace: a class's static side, an enum, an
    /// interface's members table. `getQualifiedLeftMeaning` asks for
    /// `SymbolFlagsNamespace` (`nodebuilderimpl.go:1111`).
    ContainerNotANamespace,
    /// The chain cap tripped.
    TooDeep,
}

impl Decline {
    fn label(&self) -> &'static str {
        match self {
            Decline::NoQualifierNeeded => "no qualifier needed (needsQualification = false)",
            Decline::NoContainer => "no container — Symbol.Parent unset (a namespace LOCAL)",
            Decline::ExternalModule { qualifier: Some(("alias", _)) } => {
                "container is an external module with a UNIQUE IN-SCOPE ALIAS"
            }
            Decline::ExternalModule { qualifier: Some(_) } => {
                "container is an AMBIENT module, no alias — import(\"x\") is exact"
            }
            Decline::ExternalModule { qualifier: None } => {
                "container is a FILE module, no unique alias — needs modulespecifiers"
            }
            Decline::SymbolAliasedInScope => {
                "an in-scope alias names the symbol ITSELF — bare name accessible"
            }
            Decline::ContainerNotANamespace => "container is not a namespace",
            Decline::TooDeep => "chain cap",
        }
    }
}

/// Whether an external-module symbol is **ambient** — `declare module "x"`
/// with no source-file declaration. Upstream tests the *quoted symbol name*
/// (`ast/utilities.go:1656`); this binder stores the literal's text unquoted
/// (`module_name`, `crates/tsr-binder/src/binder.rs:4091`), so the test here
/// is the declaration's shape — a first run keyed on the quotes read a false 0.
fn is_ambient_only<'a>(
    binder: &tsr_binder::BindResult<'a>,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> bool {
    let entry = binder.symbols().get(symbol);
    let has_file = entry
        .declarations
        .iter()
        .any(|&declaration| matches!(map.get(declaration), Some(Node::SourceFile(_))));
    !has_file
        && entry.declarations.iter().any(|&declaration| {
            matches!(
                map.get(declaration),
                Some(Node::ModuleDeclaration(module))
                    if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
            )
        })
}

/// The module specifier of a namespace-shaped alias declaration.
/// `examples/module_object.rs`'s `module_specifier`, reduced to the three
/// namespace shapes (an example cannot import another example).
fn namespace_alias_specifier(
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    declaration: NodeId,
) -> Option<String> {
    let literal = |expression: Option<tsr_ast::Expression<'_>>| match expression {
        Some(tsr_ast::Expression::StringLiteral(string)) => Some(string.text.to_string()),
        _ => None,
    };
    match map.get(declaration)? {
        Node::ImportEqualsDeclaration(node) => match node.module_reference {
            Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) => {
                literal(reference.expression)
            }
            _ => None,
        },
        Node::NamespaceImport(_) => {
            let mut current = nodes.parent(declaration);
            while let Some(id) = current {
                if let Some(Node::ImportDeclaration(import)) = map.get(id) {
                    return literal(import.module_specifier);
                }
                current = nodes.parent(id);
            }
            None
        }
        Node::NamespaceExport(_) => {
            let mut current = nodes.parent(declaration);
            while let Some(id) = current {
                if let Some(Node::ExportDeclaration(export)) = map.get(id) {
                    return literal(export.module_specifier);
                }
                current = nodes.parent(id);
            }
            None
        }
        _ => None,
    }
}

/// The module symbol a **namespace-shaped** alias names — the target file's
/// root symbol, or the ambient `declare module` symbol. `None` for every
/// other alias shape or an unresolvable specifier.
fn namespace_alias_target<'a>(
    program: &tsr_compiler::Program<'a>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> Option<SymbolId> {
    let &declaration = binder.symbols().get(symbol).declarations.first()?;
    let specifier = namespace_alias_specifier(nodes, map, declaration)?;
    // File resolution, the same reduction `module_object.rs` uses.
    let containing = program.source_files().iter().find(|file| file.contains(declaration))?;
    let directory = containing.file_name().rsplit_once('/').map_or("", |(head, _)| head);
    let joined =
        if directory.is_empty() { specifier.clone() } else { format!("{directory}/{specifier}") };
    let extensions =
        ["", ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", "/index.ts", "/index.d.ts"];
    for base in [joined.as_str(), specifier.as_str()] {
        for extension in extensions {
            let candidate = format!("{base}{extension}");
            if let Some(file) =
                program.source_files().iter().find(|file| file.file_name() == candidate)
                && let Some(root) = file.source_file().node_id
                && let Some(module) = binder.symbol_of(root)
            {
                return Some(binder.merged_symbol(module));
            }
        }
    }
    // The ambient fallback, as the checker's `ambient_module` does it.
    let &ambient = binder.globals().get(specifier.as_str())?;
    let ambient = binder.merged_symbol(ambient);
    is_ambient_only(binder, map, ambient).then_some(ambient)
}

/// The symbol an alias of any shape names: a namespace-shaped alias names its
/// module; an import/export **specifier** or default-import clause names the
/// target module's export of that name. `None` for shapes this reduction does
/// not follow.
fn alias_target<'a>(
    program: &tsr_compiler::Program<'a>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> Option<SymbolId> {
    fn export_name(name: tsr_ast::ModuleExportName<'_>) -> &str {
        match name {
            tsr_ast::ModuleExportName::Identifier(identifier) => identifier.text,
            tsr_ast::ModuleExportName::StringLiteral(string) => string.text,
        }
    }
    if let Some(module) = namespace_alias_target(program, binder, nodes, map, symbol) {
        return Some(module);
    }
    let &declaration = binder.symbols().get(symbol).declarations.first()?;
    let member: &str = match map.get(declaration)? {
        Node::ImportSpecifier(node) => match node.property_name {
            Some(property) => export_name(property),
            None => node.name.map(|name| name.text)?,
        },
        Node::ImportClause(_) => "default",
        Node::ExportSpecifier(node) => node.property_name.or(node.name).map(export_name)?,
        _ => return None,
    };
    // The containing module, through the same reduction the namespace shapes
    // use: give the specifier walk a NamespaceImport-like anchor by resolving
    // from the declaration itself.
    let specifier = {
        let mut current = nodes.parent(declaration);
        let mut found = None;
        while let Some(id) = current {
            match map.get(id) {
                Some(Node::ImportDeclaration(import)) => {
                    found = match import.module_specifier {
                        Some(tsr_ast::Expression::StringLiteral(string)) => {
                            Some(string.text.to_string())
                        }
                        _ => None,
                    };
                    break;
                }
                Some(Node::ExportDeclaration(export)) => {
                    found = match export.module_specifier {
                        Some(tsr_ast::Expression::StringLiteral(string)) => {
                            Some(string.text.to_string())
                        }
                        _ => None,
                    };
                    break;
                }
                _ => {}
            }
            current = nodes.parent(id);
        }
        found?
    };
    let module = resolve_module_of(program, binder, nodes, map, declaration, &specifier)?;
    let &target = binder.symbols().get(module).exports.get(member)?;
    Some(binder.merged_symbol(target))
}

/// Resolve a specifier from `declaration`'s file — a program file's root
/// symbol, or the ambient `declare module`.
fn resolve_module_of<'a>(
    program: &tsr_compiler::Program<'a>,
    binder: &tsr_binder::BindResult<'a>,
    _nodes: &NodeTable,
    map: &NodeMap<'a>,
    declaration: NodeId,
    specifier: &str,
) -> Option<SymbolId> {
    let containing = program.source_files().iter().find(|file| file.contains(declaration))?;
    let directory = containing.file_name().rsplit_once('/').map_or("", |(head, _)| head);
    let joined = if directory.is_empty() {
        specifier.to_string()
    } else {
        format!("{directory}/{specifier}")
    };
    let extensions =
        ["", ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", "/index.ts", "/index.d.ts"];
    for base in [joined.as_str(), specifier] {
        for extension in extensions {
            let candidate = format!("{base}{extension}");
            if let Some(file) =
                program.source_files().iter().find(|file| file.file_name() == candidate)
                && let Some(root) = file.source_file().node_id
                && let Some(module) = binder.symbol_of(root)
            {
                return Some(binder.merged_symbol(module));
            }
        }
    }
    let &ambient = binder.globals().get(specifier)?;
    let ambient = binder.merged_symbol(ambient);
    is_ambient_only(binder, map, ambient).then_some(ambient)
}

/// Whether any in-scope alias resolves to the **symbol itself** at `site`.
fn symbol_aliased_in_scope<'a>(
    program: &tsr_compiler::Program<'a>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    site: NodeId,
    symbol: SymbolId,
) -> bool {
    let mut current = Some(site);
    while let Some(node) = current {
        if let Some(locals) = binder.locals(node)
            && locals.values().any(|&candidate| {
                binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS)
                    && alias_target(program, binder, nodes, map, candidate) == Some(symbol)
            })
        {
            return true;
        }
        current = nodes.parent(node);
    }
    binder.globals().values().any(|&candidate| {
        binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS)
            && alias_target(program, binder, nodes, map, candidate) == Some(symbol)
    })
}

/// The name upstream's `getAccessibleSymbolChain` scope walk would print for
/// `symbol` at `site` — **the innermost table wins**, and within a table the
/// direct hit is checked before the aliases (`trySymbolTable`,
/// `symbolaccessibility.go:543` then `:562`). `Ok(name)` — which may be the
/// symbol's own name (direct hit) or an alias's; `Err(true)` = a table held
/// ≥2 distinct alias names (the chain-choice rule would decide); `Err(false)`
/// = no table in scope reaches the symbol at all.
fn best_name<'a>(
    program: &tsr_compiler::Program<'a>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    site: NodeId,
    symbol: SymbolId,
) -> Result<&'a str, bool> {
    let own = binder.symbols().get(symbol).name;
    let mut tables: Vec<&tsr_binder::SymbolTable<'a>> = Vec::new();
    let mut current = Some(site);
    while let Some(node) = current {
        if let Some(locals) = binder.locals(node) {
            tables.push(locals);
        }
        current = nodes.parent(node);
    }
    tables.push(binder.globals());
    for table in tables {
        // Direct: the table holds the symbol under its own name.
        if let Some(&hit) = table.get(own)
            && binder.merged_symbol(hit) == binder.merged_symbol(symbol)
        {
            return Ok(own);
        }
        // Aliases: any entry resolving to the symbol.
        let mut found: Option<&'a str> = None;
        for (&name, &candidate) in table {
            if !binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS) {
                continue;
            }
            if alias_target(program, binder, nodes, map, candidate)
                != Some(binder.merged_symbol(symbol))
            {
                continue;
            }
            match found {
                Some(existing) if existing == name => {}
                Some(_) => return Err(true),
                None => found = Some(name),
            }
        }
        if let Some(name) = found {
            return Ok(name);
        }
    }
    Err(false)
}

/// The unique in-scope alias name for `container` at `site` — the probe-side
/// reduction of `Checker::module_name_at`. `Ok(name)`; `Err(true)` when ≥2
/// distinct names reach it (ambiguous); `Err(false)` when none does.
fn container_alias_at<'a>(
    program: &tsr_compiler::Program<'a>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    site: NodeId,
    container: SymbolId,
) -> Result<&'a str, bool> {
    let mut candidates: Vec<SymbolId> = Vec::new();
    let mut current = Some(site);
    while let Some(node) = current {
        if let Some(locals) = binder.locals(node) {
            candidates.extend(locals.values().copied());
        }
        current = nodes.parent(node);
    }
    candidates.extend(binder.globals().values().copied());
    let mut found: Option<&'a str> = None;
    for candidate in candidates {
        if !binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS) {
            continue;
        }
        if namespace_alias_target(program, binder, nodes, map, candidate) != Some(container) {
            continue;
        }
        let name = binder.symbols().get(candidate).name;
        match found {
            Some(existing) if existing == name => {}
            Some(_) => return Err(true),
            None => found = Some(name),
        }
    }
    found.ok_or(false)
}

/// `getSymbolChain` (`nodebuilderimpl.go:1087`), reduced to the arms a build
/// would write. Returns the dotted qualifier *prefix* — `Ok("M.")`, `Ok("A.B.")`
/// — or the reason there is none.
#[allow(clippy::too_many_arguments)]
fn symbol_chain<'a>(
    program: &tsr_compiler::Program<'a>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
    site: NodeId,
    meaning: SymbolFlags,
    depth: usize,
) -> Result<String, Decline> {
    if depth >= MAX_CHAIN {
        return Err(Decline::TooDeep);
    }
    let name = binder.symbols().get(symbol).name;
    if !needs_qualification(binder, nodes, map, symbol, name, site, meaning) {
        return Err(Decline::NoQualifierNeeded);
    }
    // `getContainersOfSymbol` (`symbolaccessibility.go:280`) — its first and
    // normal answer, `getParentOfSymbol` (`checker.go:14365`).
    let Some(parent) = binder.symbols().get(symbol).parent.map(|p| binder.merged_symbol(p)) else {
        return Err(Decline::NoContainer);
    };
    if is_external_module(binder, map, parent) {
        // `trySymbolTable`'s direct arm first: an in-scope alias naming the
        // symbol ITSELF makes the bare name accessible, and no qualifier may
        // fire — this is what keeps the moduleAugmentation right-lines right.
        if symbol_aliased_in_scope(program, binder, nodes, map, site, symbol) {
            return Err(Decline::SymbolAliasedInScope);
        }
        // The container-qualifier design's two arms, in upstream's order:
        // `getAccessibleSymbolChain` first (an alias in scope), the
        // `ImportTypeNode` specifier second — exact only for an ambient module.
        let arm = match container_alias_at(program, binder, nodes, map, site, parent) {
            Ok(alias) => Some(("alias", format!("{alias}."))),
            // Ambiguity keeps the shipped refusal: no guess, no import-form
            // fallback either, since upstream would have picked an alias.
            Err(true) => None,
            Err(false) => is_ambient_only(binder, map, parent).then(|| {
                let name = binder.symbols().get(parent).name;
                ("ambient-import", format!("import(\"{name}\")."))
            }),
        };
        return Err(Decline::ExternalModule { qualifier: arm });
    }
    if !binder.symbols().get(parent).flags.intersects(SymbolFlags::MODULE) {
        return Err(Decline::ContainerNotANamespace);
    }
    let parent_name = binder.symbols().get(parent).name;
    // `getQualifiedLeftMeaning(meaning)` is `SymbolFlagsNamespace`
    // (`nodebuilderimpl.go:1111`).
    match symbol_chain(program, binder, nodes, map, parent, site, SymbolFlags::NAMESPACE, depth + 1)
    {
        Ok(prefix) => Ok(format!("{prefix}{parent_name}.")),
        // The parent itself does not need qualifying: the chain stops there,
        // which is the recursion's base case and not a decline.
        Err(Decline::NoQualifierNeeded) => Ok(format!("{parent_name}.")),
        // A module-container stop above extends its forecast prefix on unwind,
        // so the top-level caller holds e.g. `import("x").N.` or `m4.N.`.
        Err(Decline::ExternalModule { qualifier: Some((arm, prefix)) }) => {
            Err(Decline::ExternalModule {
                qualifier: Some((arm, format!("{prefix}{parent_name}."))),
            })
        }
        Err(other) => Err(other),
    }
}

/// The nearest enclosing `ModuleDeclaration` of any of a symbol's declarations
/// — the container definition `qualname.rs` used. CP6's, and looser than
/// upstream's by exactly the non-exported locals.
fn ast_namespace_container<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    symbol: SymbolId,
) -> Option<NodeId> {
    for &declaration in &binder.symbols().get(symbol).declarations {
        let mut current = nodes.parent(declaration);
        while let Some(node) = current {
            if matches!(map.get(node), Some(Node::ModuleDeclaration(_))) {
                return Some(node);
            }
            current = nodes.parent(node);
        }
    }
    None
}

/// Whether `node` is inside `container`.
fn is_inside(nodes: &NodeTable, node: NodeId, container: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == container {
            return true;
        }
        current = nodes.parent(id);
    }
    false
}

/// The identifier-shaped tokens of a printed type, for the loose bound.
fn name_tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|token| !token.is_empty() && !token.starts_with(|c: char| c.is_ascii_digit()))
        .collect()
}

/// Does `printed` contain `A.B` where `A` resolves as a namespace at `site`?
/// CP7's predicate — design W's own output, which is what makes this run
/// post-W rather than nominally post-W.
fn dotted_namespace_root<'a>(
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    site: NodeId,
    printed: &str,
) -> bool {
    let bytes = printed.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    let mut start = None;
    for (index, &byte) in bytes.iter().enumerate() {
        if ident(byte) {
            start.get_or_insert(index);
            continue;
        }
        if let Some(from) = start.take()
            && byte == b'.'
            && bytes.get(index + 1).is_some_and(|&next| ident(next))
            && !bytes[from].is_ascii_digit()
            && binder
                .resolve_name(nodes, map, site, &printed[from..index], SymbolFlags::NAMESPACE)
                .is_some()
        {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// The report.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Report {
    aligned: usize,
    right: usize,
    wrong: usize,
    gap: usize,

    // --- strict: P rewrites a line whose whole printed answer IS a name -----
    /// Right lines where the strict gate holds and a container exists at all —
    /// the denominator the at-risk figure is a share of.
    p_denominator: usize,
    converts: usize,
    converts_lines: BTreeMap<String, usize>,
    converts_cases: BTreeMap<String, usize>,
    churn: usize,
    churn_lines: BTreeMap<String, usize>,
    churn_cases: BTreeMap<String, usize>,
    at_risk: usize,
    at_risk_lines: BTreeMap<String, usize>,
    at_risk_cases: BTreeMap<String, usize>,
    /// `(sub-family, outcome)` — what a positional refusal costs and saves.
    families: BTreeMap<(String, &'static str), usize>,
    declines: BTreeMap<&'static str, usize>,

    // --- the container-qualifier design's counterfactual, same pass ---------
    /// `(arm, outcome)` — the alias arm and the ambient-import arm, each with
    /// CONVERTS / WOULD-WRONG / AT RISK / NO-OP / GAP.
    arm_outcomes: BTreeMap<(&'static str, &'static str), usize>,
    arm_lines: BTreeMap<String, usize>,
    /// The RENAME design — `best_name`'s innermost-table walk producing a
    /// DIFFERENT name than the symbol's own, over every strict-gate line.
    rename_outcomes: BTreeMap<&'static str, usize>,
    rename_lines: BTreeMap<String, usize>,
    /// The FILE half, split by what the baseline wants: `import(` — needs the
    /// `modulespecifiers` package — versus anything else, which an accessible
    /// **alias** could in principle spell without one.
    file_want_import: usize,
    file_want_other: usize,
    file_lines: BTreeMap<String, usize>,

    // --- loose: token rewriting inside composite prints ---------------------
    loose_converts: usize,
    loose_converts_lines: BTreeMap<String, usize>,
    loose_churn: usize,
    loose_at_risk: usize,
    loose_at_risk_cases: BTreeMap<String, usize>,
    loose_at_risk_lines: BTreeMap<String, usize>,

    // --- controls -----------------------------------------------------------
    cp1_type_parameter_qualified: usize,
    cp2_chain_past_stop: usize,
    cp4_hits: BTreeMap<String, usize>,
    cp5_parser_real_source_10: usize,
    cp6_ast_walk_at_risk: usize,
    /// CP6 again, with `needs_qualification` comparing symbol ids **without**
    /// `getMergedSymbol` — which is what `qualname.rs` does. The difference
    /// between the two is the merged-declaration population.
    cp6_unmerged: usize,
    cp6_unmerged_lines: BTreeMap<String, usize>,
    /// Every line in one of CP4's named cases whose printed answer is a name P
    /// looked at, and what became of it.
    cp4_audit: BTreeMap<String, usize>,
    cp7_right_dotted: usize,
    /// Gap lines the strict path reached. Zero by construction — `error` names
    /// no symbol — and reported so the construction claim is measured.
    gap_reached: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        for (target, source) in [
            (&mut self.aligned, other.aligned),
            (&mut self.right, other.right),
            (&mut self.wrong, other.wrong),
            (&mut self.gap, other.gap),
            (&mut self.p_denominator, other.p_denominator),
            (&mut self.converts, other.converts),
            (&mut self.churn, other.churn),
            (&mut self.at_risk, other.at_risk),
            (&mut self.loose_converts, other.loose_converts),
            (&mut self.loose_churn, other.loose_churn),
            (&mut self.loose_at_risk, other.loose_at_risk),
            (&mut self.cp1_type_parameter_qualified, other.cp1_type_parameter_qualified),
            (&mut self.cp2_chain_past_stop, other.cp2_chain_past_stop),
            (&mut self.cp5_parser_real_source_10, other.cp5_parser_real_source_10),
            (&mut self.cp6_ast_walk_at_risk, other.cp6_ast_walk_at_risk),
            (&mut self.cp6_unmerged, other.cp6_unmerged),
            (&mut self.cp7_right_dotted, other.cp7_right_dotted),
            (&mut self.gap_reached, other.gap_reached),
            (&mut self.file_want_import, other.file_want_import),
            (&mut self.file_want_other, other.file_want_other),
        ] {
            *target += source;
        }
        for (key, n) in &other.declines {
            *self.declines.entry(key).or_default() += n;
        }
        for (key, n) in &other.families {
            *self.families.entry(key.clone()).or_default() += n;
        }
        for (&key, n) in &other.arm_outcomes {
            *self.arm_outcomes.entry(key).or_default() += n;
        }
        for (&key, n) in &other.rename_outcomes {
            *self.rename_outcomes.entry(key).or_default() += n;
        }
        for (target, source) in [
            (&mut self.converts_lines, &other.converts_lines),
            (&mut self.converts_cases, &other.converts_cases),
            (&mut self.churn_lines, &other.churn_lines),
            (&mut self.churn_cases, &other.churn_cases),
            (&mut self.at_risk_lines, &other.at_risk_lines),
            (&mut self.at_risk_cases, &other.at_risk_cases),
            (&mut self.loose_converts_lines, &other.loose_converts_lines),
            (&mut self.loose_at_risk_cases, &other.loose_at_risk_cases),
            (&mut self.loose_at_risk_lines, &other.loose_at_risk_lines),
            (&mut self.cp4_hits, &other.cp4_hits),
            (&mut self.cp4_audit, &other.cp4_audit),
            (&mut self.cp6_unmerged_lines, &other.cp6_unmerged_lines),
            (&mut self.arm_lines, &other.arm_lines),
            (&mut self.rename_lines, &other.rename_lines),
            (&mut self.file_lines, &other.file_lines),
        ] {
            for (key, n) in source {
                *target.entry(key.clone()).or_default() += n;
            }
        }
    }
}

/// The strict gate: is `printed` this symbol's own name, possibly with type
/// arguments or under `typeof`? Returns the prefix and suffix around the name.
///
/// `checker-notes-nameres.md` §49 records this gate as the design: *"Gated so
/// that it applies only where the printed form IS the symbol's name … because
/// without that gate it climbs from an anonymous `__function` symbol to no
/// parent and gaps every function and object type in the corpus."*
fn split_around_name<'t>(printed: &'t str, name: &str) -> Option<(&'static str, &'t str)> {
    if name.is_empty() {
        return None;
    }
    if let Some(rest) = printed.strip_prefix("typeof ") {
        return if rest == name { Some(("typeof ", "")) } else { None };
    }
    if printed == name {
        return Some(("", ""));
    }
    printed.strip_prefix(name).filter(|rest| rest.starts_with('<')).map(|rest| ("", rest))
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
    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));

    // The loose bound's index: every symbol the node builder could *name* as a
    // type and that has a namespace container. `lookupSymbolChainWorker`
    // excludes type parameters by construction (`nodebuilderimpl.go:1069`), and
    // `qualname.rs` §5 records what happens without that restriction — the
    // bound's head became `number`, `this`, `T`, `value` and it read 111,321.
    let mut namespaced: HashMap<&str, Vec<SymbolId>> = HashMap::new();
    for (id, symbol) in bound.symbols().iter() {
        if symbol.flags.intersects(SymbolFlags::TYPE)
            && !symbol.flags.intersects(SymbolFlags::TYPE_PARAMETER)
            && symbol
                .parent
                .map(|p| bound.merged_symbol(p))
                .is_some_and(|p| bound.symbols().get(p).flags.intersects(SymbolFlags::MODULE))
        {
            namespaced.entry(symbol.name).or_default().push(id);
        }
    }

    let mut report = Report::default();
    let short = case.name.rsplit_once('/').map_or(case.name.as_str(), |(_, n)| n);
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
                // Unaligned: the two sides disagree about the node text, so no
                // comparison of answers is meaningful.
                continue;
            }
            report.aligned += 1;
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();
            let printed = got.type_string.clone();
            let is_gap = !is_right && printed == "error";
            if is_right {
                report.right += 1;
                if dotted_namespace_root(bound, nodes, map, id, &printed) {
                    report.cp7_right_dotted += 1;
                }
            } else if is_gap {
                report.gap += 1;
            } else {
                report.wrong += 1;
            }

            // ---------------------------------------------------------------
            // STRICT — P rewrites a line whose whole printed answer IS a name.
            // ---------------------------------------------------------------
            let type_id = types_producer::type_id_at_location(&mut checker, bound, nodes, map, id);
            let named = match &checker.type_of(type_id).data {
                TypeData::Named { text, members } => Some((text.clone(), *members)),
                TypeData::Anonymous { text, symbol, .. } => Some((text.clone(), Some(*symbol))),
                _ => None,
            };
            // CP6, replicating `qualname.rs`'s at-risk-P block **verbatim** so
            // the two runs are comparable: its gate reads the `TypeData` text,
            // not the printed assertion line, and its container is the AST walk.
            if is_right && let Some((data_text, Some(symbol))) = named.clone() {
                let name = bound.symbols().get(symbol).name;
                let prints_as_the_name = data_text == name
                    || data_text.starts_with(&format!("{name}<"))
                    || data_text == format!("typeof {name}");
                if prints_as_the_name
                    && ast_namespace_container(bound, nodes, map, symbol).is_some()
                {
                    let meaning = SymbolFlags::TYPE | SymbolFlags::VALUE;
                    let resolved = bound.resolve_name(nodes, map, id, name, meaning);
                    // `qualname.rs`'s form: a raw id comparison.
                    if resolved != Some(symbol) {
                        report.cp6_unmerged += 1;
                        // The lines the merged comparison drops — upstream's
                        // `needsQualification` calls `getMergedSymbol` on the
                        // table hit (`symbolaccessibility.go:702`) before the
                        // identity test, so these are lines upstream does NOT
                        // qualify and `qualname.rs` counted anyway.
                        if resolved.map(|r| bound.merged_symbol(r))
                            == Some(bound.merged_symbol(symbol))
                        {
                            *report
                                .cp6_unmerged_lines
                                .entry(format!("{}  [{}]", want.text, case.name))
                                .or_default() += 1;
                        }
                    }
                    if needs_qualification(bound, nodes, map, symbol, name, id, meaning) {
                        report.cp6_ast_walk_at_risk += 1;
                    }
                }
            }

            if let Some((_, Some(symbol))) = named {
                let name = bound.symbols().get(symbol).name;
                if let Some((prefix, suffix)) = split_around_name(&printed, name) {
                    if is_gap {
                        report.gap_reached += 1;
                    }
                    // The RENAME design — the innermost-table walk of
                    // `getAccessibleSymbolChain` producing a *different* name
                    // than the symbol's own (an inner alias shadowing an outer
                    // direct hit). Priced over every strict-gate line, because
                    // its at-risk population is every RIGHT line whose bare
                    // name upstream also chose.
                    if !is_gap
                        && let Ok(better) = best_name(&program, bound, nodes, map, id, symbol)
                        && better != name
                    {
                        let forecast = format!("{prefix}{better}{suffix}");
                        let outcome = if is_right {
                            "AT RISK"
                        } else if forecast == wanted {
                            "CONVERTS"
                        } else {
                            "WOULD-WRONG"
                        };
                        *report.rename_outcomes.entry(outcome).or_default() += 1;
                        *report
                            .rename_lines
                            .entry(format!(
                                "{outcome:<11} want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                                case.name
                            ))
                            .or_default() += 1;
                    }
                    let meaning = SymbolFlags::TYPE | SymbolFlags::VALUE;
                    let has_container = bound.symbols().get(symbol).parent.is_some();
                    match symbol_chain(&program, bound, nodes, map, symbol, id, meaning, 0) {
                        Err(reason) => {
                            *report.declines.entry(reason.label()).or_default() += 1;
                            // The container-qualifier design, priced in the
                            // same pass: the forecast is a whole string, so
                            // CONVERTS / WOULD-WRONG / AT-RISK all come from
                            // one comparison.
                            if let Decline::ExternalModule {
                                qualifier: Some((arm, ref qualifier)),
                            } = reason
                            {
                                let forecast = format!("{prefix}{qualifier}{name}{suffix}");
                                let outcome = if forecast == printed {
                                    "NO-OP"
                                } else if is_right {
                                    "AT RISK"
                                } else if is_gap {
                                    "GAP"
                                } else if forecast == wanted {
                                    "CONVERTS"
                                } else {
                                    "WOULD-WRONG"
                                };
                                *report.arm_outcomes.entry((arm, outcome)).or_default() += 1;
                                *report
                                    .arm_lines
                                    .entry(format!(
                                        "{arm:<14} {outcome:<11} want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                            }
                            // The FILE half: what would upstream print here?
                            // `import(` in the want means only the
                            // `modulespecifiers` package can spell it; anything
                            // else an accessible alias might.
                            if let Decline::ExternalModule { qualifier: None } = reason {
                                let verdict = if is_right {
                                    "right"
                                } else if is_gap {
                                    "gap  "
                                } else {
                                    "wrong"
                                };
                                if wanted.contains("import(") {
                                    report.file_want_import += 1;
                                } else {
                                    report.file_want_other += 1;
                                }
                                *report
                                    .file_lines
                                    .entry(format!(
                                        "{verdict} want `{wanted}`, printed `{printed}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                            }
                        }
                        Ok(qualifier) => {
                            debug_assert!(has_container);
                            report.p_denominator += 1;
                            let forecast = format!("{prefix}{qualifier}{name}{suffix}");
                            // CP1 — upstream never qualifies a type parameter.
                            if bound
                                .symbols()
                                .get(symbol)
                                .flags
                                .intersects(SymbolFlags::TYPE_PARAMETER)
                            {
                                report.cp1_type_parameter_qualified += 1;
                            }
                            // CP2 — the chain must not be built past its stop
                            // condition.
                            if forecast != printed
                                && !needs_qualification(
                                    bound, nodes, map, symbol, name, id, meaning,
                                )
                            {
                                report.cp2_chain_past_stop += 1;
                            }

                            // The sub-families a positional refusal is priced
                            // against.
                            let parent =
                                bound.symbols().get(symbol).parent.map(|p| bound.merged_symbol(p));
                            let site_inside = parent.is_some_and(|p| {
                                bound
                                    .symbols()
                                    .get(p)
                                    .declarations
                                    .iter()
                                    .any(|&d| is_inside(nodes, id, d))
                            });
                            let in_lib = bound
                                .symbols()
                                .get(symbol)
                                .declarations
                                .iter()
                                .any(|&d| program.lib_files().iter().any(|f| f.contains(d)));
                            let flags = bound.symbols().get(symbol).flags;
                            let kind = if flags.intersects(SymbolFlags::ENUM_MEMBER) {
                                "enum member  "
                            } else if flags.intersects(SymbolFlags::ENUM) {
                                "enum         "
                            } else if flags.intersects(SymbolFlags::CLASS) {
                                "class        "
                            } else if flags.intersects(SymbolFlags::INTERFACE) {
                                "interface    "
                            } else if flags.intersects(SymbolFlags::TYPE_ALIAS) {
                                "type alias   "
                            } else if flags.intersects(SymbolFlags::MODULE) {
                                "namespace    "
                            } else {
                                "other        "
                            };
                            let family = format!(
                                "{} · {} · {} · {} segment(s) · {}",
                                kind,
                                if in_lib { "lib " } else { "user" },
                                if site_inside {
                                    "site INSIDE the container"
                                } else {
                                    "site outside            "
                                },
                                qualifier.matches('.').count(),
                                if suffix.is_empty() && prefix.is_empty() {
                                    "bare name     "
                                } else if prefix.is_empty() {
                                    "type arguments"
                                } else {
                                    "under `typeof`"
                                },
                            );

                            let outcome = if forecast == printed {
                                "NO-OP"
                            } else if is_right {
                                report.at_risk += 1;
                                *report
                                    .at_risk_lines
                                    .entry(format!("{printed}  ->  {forecast}  [{}]", case.name))
                                    .or_default() += 1;
                                *report.at_risk_cases.entry(case.name.clone()).or_default() += 1;
                                if case.name == CP5_CASE {
                                    report.cp5_parser_real_source_10 += 1;
                                }
                                "AT RISK"
                            } else if is_gap {
                                "GAP"
                            } else if forecast == wanted {
                                report.converts += 1;
                                *report
                                    .converts_lines
                                    .entry(format!(
                                        "want `{wanted}`, from `{printed}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                                *report.converts_cases.entry(case.name.clone()).or_default() += 1;
                                if CP4_CASES.contains(&short) {
                                    *report.cp4_hits.entry(case.name.clone()).or_default() += 1;
                                }
                                "CONVERTS"
                            } else {
                                report.churn += 1;
                                *report.churn_cases.entry(case.name.clone()).or_default() += 1;
                                *report
                                    .churn_lines
                                    .entry(format!(
                                        "want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                                "CHURN"
                            };
                            *report.families.entry((family, outcome)).or_default() += 1;
                            if CP4_CASES.contains(&short) {
                                *report
                                    .cp4_audit
                                    .entry(format!(
                                        "{outcome:<9} want `{wanted}`, `{printed}` -> `{forecast}`  [{}]",
                                        case.name
                                    ))
                                    .or_default() += 1;
                            }
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            // LOOSE — token rewriting inside composite prints. An upper bound
            // on a FULLY qualifying node builder, not on the design above.
            // ---------------------------------------------------------------
            let mut loose = printed.clone();
            let mut changed = false;
            for token in name_tokens(&printed) {
                let Some(candidates) = namespaced.get(token) else { continue };
                let Some(&symbol) = candidates.iter().find(|&&symbol| {
                    needs_qualification(
                        bound,
                        nodes,
                        map,
                        symbol,
                        token,
                        id,
                        SymbolFlags::TYPE | SymbolFlags::VALUE,
                    )
                }) else {
                    continue;
                };
                // The shipped strict path's INSIDE refusal (`bd tsr-2ghn`'s
                // refinement): a reference site inside the container it would
                // be qualified by takes no qualifier — the at-risk head
                // (`privacy*`, `publicClass` inside `publicModule`) is this
                // family, and the strict build keeps the decline at 1.2:1.
                let site_inside =
                    bound.symbols().get(symbol).parent.map(|p| bound.merged_symbol(p)).is_some_and(
                        |p| {
                            bound
                                .symbols()
                                .get(p)
                                .declarations
                                .iter()
                                .any(|&declaration| is_inside(nodes, id, declaration))
                        },
                    );
                if site_inside {
                    continue;
                }
                let Ok(qualifier) = symbol_chain(
                    &program,
                    bound,
                    nodes,
                    map,
                    symbol,
                    id,
                    SymbolFlags::TYPE | SymbolFlags::VALUE,
                    0,
                ) else {
                    continue;
                };
                // Whole-token replacement only.
                let replaced = replace_token(&loose, token, &format!("{qualifier}{token}"));
                if replaced != loose {
                    loose = replaced;
                    changed = true;
                }
            }
            if changed {
                if is_right {
                    report.loose_at_risk += 1;
                    *report.loose_at_risk_cases.entry(case.name.clone()).or_default() += 1;
                    *report
                        .loose_at_risk_lines
                        .entry(format!("{printed}  ->  {loose}  [{}]", case.name))
                        .or_default() += 1;
                } else if !is_gap {
                    if loose == wanted {
                        report.loose_converts += 1;
                        *report
                            .loose_converts_lines
                            .entry(format!("want `{wanted}`, from `{printed}`  [{}]", case.name))
                            .or_default() += 1;
                    } else {
                        report.loose_churn += 1;
                    }
                }
            }
        }
    }
    Some(report)
}

/// Replace whole identifier tokens equal to `token`, leaving `A.token` and
/// `tokenSuffix` alone.
fn replace_token(text: &str, token: &str, with: &str) -> String {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '.';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(offset) = rest.find(token) {
        let before = &rest[..offset];
        let after = &rest[offset + token.len()..];
        let ok = before.chars().next_back().is_none_or(|c| !ident(c))
            && after.chars().next().is_none_or(|c| !ident(c));
        out.push_str(before);
        out.push_str(if ok { with } else { token });
        rest = after;
    }
    out.push_str(rest);
    out
}

#[allow(clippy::cast_precision_loss)]
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 { 0.0 } else { 100.0 * part as f64 / whole as f64 }
}

fn print_map(title: &str, map: &BTreeMap<String, usize>, take: usize) {
    println!("\n## {title}\n");
    let mut rows: Vec<_> = map.iter().collect();
    rows.sort_by_key(|(key, n)| (std::cmp::Reverse(**n), (*key).clone()));
    let total: usize = map.values().sum();
    for (key, n) in rows.iter().take(take) {
        println!("  {n:>6}  {key}");
    }
    match rows.first() {
        Some((key, n)) => {
            println!("  -- total {total}, top-1 {n} = {:.1}%  ({key})", percent(**n, total));
        }
        None => println!("  -- total 0"),
    }
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]
fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");
    let mut report = Report::default();
    for partial in cases.par_iter().filter_map(measure).collect::<Vec<_>>() {
        report.merge(&partial);
    }

    println!("# qualnamep — design P re-sized against the compiler design W left behind\n");
    println!(
        "aligned lines:  right {}  gap {}  wrong {}   (total {})",
        report.right, report.gap, report.wrong, report.aligned
    );

    println!("\n## The three columns — STRICT (the design cycle 20b built)\n");
    println!("  CONVERTS   wrong today, P's text IS the baseline's      {:>7}", report.converts);
    println!("  CHURN      wrong today, P changes it, still not right   {:>7}", report.churn);
    println!("  AT RISK    RIGHT today, P changes the text — LOST       {:>7}", report.at_risk);
    println!(
        "  ---------  net if built                                {:>7}",
        report.converts as i64 - report.at_risk as i64
    );
    println!(
        "  ratio      gained per lost                              {:>7.2}",
        report.converts as f64 / report.at_risk.max(1) as f64
    );
    println!(
        "  denominator (strict gate holds and a chain was built)   {:>7}   at-risk share {:.2}%",
        report.p_denominator,
        percent(report.at_risk, report.p_denominator)
    );
    println!(
        "  cases:  converting {}, at risk {}",
        report.converts_cases.len(),
        report.at_risk_cases.len()
    );

    println!("\n## The three columns — LOOSE (a FULLY qualifying node builder; NOT this design)\n");
    println!("  CONVERTS  {:>7}", report.loose_converts);
    println!("  CHURN     {:>7}", report.loose_churn);
    println!(
        "  AT RISK   {:>7}   in {} cases",
        report.loose_at_risk,
        report.loose_at_risk_cases.len()
    );
    println!("  net       {:>7}", report.loose_converts as i64 - report.loose_at_risk as i64);

    println!("\n## Controls\n");
    println!(
        "  CP1  type parameters qualified                {:>7}   (expect 0, nodebuilderimpl.go:1069)",
        report.cp1_type_parameter_qualified
    );
    println!(
        "  CP2  chain built past its stop condition      {:>7}   (expect 0, nodebuilderimpl.go:1094)",
        report.cp2_chain_past_stop
    );
    println!(
        "  CP3  buckets {} + {} + {} + no-op = strict gate hits {}",
        report.converts, report.churn, report.at_risk, report.p_denominator
    );
    println!(
        "  CP4  conversions in §9.3's named residual cases {:>5}   (expect >= 13)",
        report.cp4_hits.values().sum::<usize>()
    );
    for (case, n) in &report.cp4_hits {
        println!("         {n:>5}  {case}");
    }
    println!(
        "  CP5  at-risk lines in {CP5_CASE} {:>4}   (expect 0; was -481)",
        report.cp5_parser_real_source_10
    );
    println!(
        "  CP6  at-risk under qualname.rs's AST-walk container {:>4}   (expect >= 36, it read 36 pre-W)",
        report.cp6_ast_walk_at_risk
    );
    println!(
        "       ... same, with qualname.rs's UNMERGED id comparison  {:>4}   (this is what 36 was)",
        report.cp6_unmerged
    );
    println!(
        "  CP7  right lines printing a dotted namespace name {:>6}   (expect > 0 — proves this tree HAS design W)",
        report.cp7_right_dotted
    );
    println!(
        "  --   gap lines the strict path reached        {:>7}   (expect 0 — `error` names no symbol)",
        report.gap_reached
    );

    println!("\n## Why a chain was NOT built\n");
    let total: usize = report.declines.values().sum();
    let mut declines: Vec<_> = report.declines.iter().collect();
    declines.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (reason, n) in declines {
        println!("  {n:>7}  {reason}");
    }
    println!("  -- total {total}");

    println!("\n## The container-qualifier design — per arm, per outcome\n");
    for ((arm, outcome), n) in &report.arm_outcomes {
        println!("  {arm:<16} {outcome:<12} {n:>7}");
    }
    print_map("container-qualifier forecast, verbatim", &report.arm_lines, 60);

    println!("\n## The RENAME design — best_name differs from the symbol's own\n");
    for (outcome, n) in &report.rename_outcomes {
        println!("  {outcome:<12} {n:>7}");
    }
    print_map("rename forecast, verbatim", &report.rename_lines, 40);
    println!("\n## bd tsr-xpb8 — the FILE half, by what the baseline wants\n");
    println!(
        "  want contains `import(`  (modulespecifiers only)        {:>7}",
        report.file_want_import
    );
    println!(
        "  want is anything else    (an alias might spell it)      {:>7}",
        report.file_want_other
    );
    print_map("bd tsr-xpb8 — FILE half, verbatim", &report.file_lines, 50);

    print_map(
        "CP6 — the lines getMergedSymbol drops (qualname.rs counted them)",
        &report.cp6_unmerged_lines,
        25,
    );
    print_map("CP4 — every P-visited line in §9.3's named cases", &report.cp4_audit, 40);
    print_map("CONVERTS — verbatim", &report.converts_lines, 30);
    print_map("CONVERTS — by case", &report.converts_cases, 20);
    print_map("AT RISK — verbatim", &report.at_risk_lines, 30);
    print_map("AT RISK — by case", &report.at_risk_cases, 20);
    print_map("CHURN — verbatim", &report.churn_lines, 20);
    print_map("CHURN — by case", &report.churn_cases, 20);
    print_map("LOOSE CONVERTS — verbatim", &report.loose_converts_lines, 20);
    print_map("LOOSE AT RISK — verbatim", &report.loose_at_risk_lines, 20);
    print_map("LOOSE AT RISK — by case", &report.loose_at_risk_cases, 20);

    println!("\n## The sub-family split — what a positional refusal would cost and save\n");
    let mut families: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    for ((family, outcome), n) in &report.families {
        *families.entry(family.clone()).or_default().entry(outcome).or_default() += n;
    }
    let mut ordered: Vec<_> = families.iter().collect();
    ordered.sort_by_key(|(_, cells)| {
        std::cmp::Reverse(
            cells.get("CONVERTS").copied().unwrap_or(0)
                + cells.get("AT RISK").copied().unwrap_or(0),
        )
    });
    println!(
        "  {:<78}  {:>8} {:>8} {:>7} {:>7}",
        "sub-family", "CONVERTS", "AT RISK", "CHURN", "NO-OP"
    );
    for (family, cells) in ordered {
        let get = |outcome| cells.get(outcome).copied().unwrap_or(0);
        if get("CONVERTS") + get("AT RISK") + get("CHURN") == 0 {
            continue;
        }
        println!(
            "  {family:<78}  {:>8} {:>8} {:>7} {:>7}",
            get("CONVERTS"),
            get("AT RISK"),
            get("CHURN"),
            get("NO-OP")
        );
    }
}
