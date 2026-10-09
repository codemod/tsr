//! TS2308 — `Module {0} has already exported a member named '{1}'. Consider
//! explicitly re-exporting to resolve the ambiguity.`
//!
//! Upstream raises it while building a module's export table:
//! `getExportsOfModuleWorker` (`checker.go:16148`) visits every `export *`,
//! merges the re-exported tables with `extendExportSymbols`
//! (`checker.go:16235`), and reports each `export *` that brings a name a
//! previous star already brought with a different resolved symbol, unless
//! the module exports that name itself. `checkExternalModuleExports` is what
//! forces the table for a checked module.
//!
//! # Checker port boundary
//!
//! - **Native operation:** `getExportsOfModule` → `getExportsOfModuleWorker`,
//!   cached per module symbol in `moduleSymbolLinks.resolvedExports`.
//! - **No cache or side table here.** This port answers export lookups per
//!   name (`Checker::get_export_from_star`, `symbols.rs`, which records why);
//!   the whole table is built only here, once per checked module file that
//!   has an `export *` (a module without one has no collision table), on a
//!   short-lived map dropped at the end of the call. Key identity is the
//!   binder's module `SymbolId`; nothing is published.
//! - **Traversal:** `visit` over `export *` declarations with upstream's
//!   visited list, so a cycle of stars terminates exactly as upstream's does.
//!   The expensive work is the module-name resolution of each star, which is
//!   the host lookup every other star reader already does.
//! - **Reporting:** upstream's recursion reports every visited module's
//!   collisions, and its diagnostic collection deduplicates the repeats.
//!   Here only the checked module's own level reports; a nested module's
//!   collisions are reported when that module is checked. The two differ
//!   only for a module that is visited but never checked.

use std::collections::HashMap;

use tsr_ast::{Node, NodeId};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_core::Span;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `ast.InternalSymbolNameExportStar`; `symbols.rs` spells it the same way.
const EXPORT_STAR: &str = "__export";
/// `ast.InternalSymbolNameExportEquals`.
const EXPORT_EQUALS: &str = "export=";
/// `ast.InternalSymbolNameDefault`.
const DEFAULT: &str = "default";

/// `ExportCollision` (`checker.go:16140`): the first star's specifier text
/// and every later star that brought a different symbol under the name.
struct ExportCollision {
    specifier_text: String,
    exports_with_duplicate: Vec<NodeId>,
}

impl Checker<'_, '_> {
    /// `checkExternalModuleExports`' `getExportsOfModule(moduleSymbol)` call
    /// (`checker.go:5672`), for the TS2308 reports it produces.
    pub(crate) fn check_export_star_conflicts(&mut self, file: NodeId) {
        let Some(module) = self.binder.symbol_of(file) else { return };
        let module = self.binder.merged_symbol(module);
        // A module defined by `export =` is that one export
        // (`resolveExternalModuleSymbol`); its own stars are not visited.
        if self.binder.symbols().get(module).exports.contains_key(EXPORT_EQUALS) {
            return;
        }
        // Only a module with `export *` declarations has a collision table;
        // without one the worker's table is the module's own exports and
        // reports nothing, so it is not built.
        if !self.binder.symbols().get(module).exports.contains_key(EXPORT_STAR) {
            return;
        }
        let mut visited = Vec::new();
        self.visit_export_stars(module, &mut visited, true);
    }

    /// `getExportsOfModuleWorker`'s `visit` closure, without the type-only
    /// bookkeeping, which is not a TS2308 input.
    fn visit_export_stars(
        &mut self,
        module: SymbolId,
        visited: &mut Vec<SymbolId>,
        report: bool,
    ) -> Option<HashMap<String, SymbolId>> {
        if visited.contains(&module) || !self.binder.symbols().get(module).exports.is_present() {
            return None;
        }
        visited.push(module);
        let entry = self.binder.symbols().get(module);
        let mut symbols: HashMap<String, SymbolId> =
            entry.exports.iter().map(|(name, &symbol)| ((*name).to_string(), symbol)).collect();
        let Some(&stars) = entry.exports.get(EXPORT_STAR) else { return Some(symbols) };
        let stars: Vec<NodeId> = self.binder.symbols().get(stars).declarations.to_vec();
        let mut nested: HashMap<String, SymbolId> = HashMap::new();
        let mut lookup: HashMap<String, ExportCollision> = HashMap::new();
        for declaration in stars {
            let Some(Node::ExportDeclaration(export)) = self.node_map.get(declaration) else {
                continue;
            };
            let Some(specifier) = export.module_specifier.and_then(|s| Node::from(s).node_id())
            else {
                continue;
            };
            let exported = match self.resolve_external_module_name(declaration, specifier) {
                Some(target) => {
                    let target = self.binder.merged_symbol(target);
                    self.visit_export_stars(target, visited, false)
                }
                None => None,
            };
            if let Some(exported) = exported {
                let specifier_text = self.export_star_specifier_text(specifier);
                self.extend_export_symbols(
                    &mut nested,
                    exported,
                    Some((&mut lookup, declaration, &specifier_text)),
                );
            }
        }
        if report {
            let mut reports: Vec<(NodeId, String, String)> = Vec::new();
            for (name, collision) in &lookup {
                if name == EXPORT_EQUALS
                    || collision.exports_with_duplicate.is_empty()
                    || symbols.contains_key(name)
                {
                    continue;
                }
                for &node in &collision.exports_with_duplicate {
                    reports.push((node, collision.specifier_text.clone(), name.clone()));
                }
            }
            for (node, specifier_text, name) in reports {
                self.report_export_star_conflict(node, specifier_text, name);
            }
        }
        self.extend_export_symbols(&mut symbols, nested, None);
        Some(symbols)
    }

    /// `extendExportSymbols` (`checker.go:16235`).
    fn extend_export_symbols(
        &mut self,
        target: &mut HashMap<String, SymbolId>,
        source: HashMap<String, SymbolId>,
        mut lookup: Option<(&mut HashMap<String, ExportCollision>, NodeId, &str)>,
    ) {
        for (name, source_symbol) in source {
            if name == DEFAULT {
                continue;
            }
            match target.get(&name).copied() {
                None => {
                    if let Some((table, _, specifier_text)) = lookup.as_mut() {
                        table.insert(
                            name.clone(),
                            ExportCollision {
                                specifier_text: (*specifier_text).to_string(),
                                exports_with_duplicate: Vec::new(),
                            },
                        );
                    }
                    target.insert(name, source_symbol);
                }
                Some(target_symbol) => {
                    let Some((table, export_node, _)) = lookup.as_mut() else { continue };
                    if self.resolve_symbol_for_export(target_symbol)
                        != self.resolve_symbol_for_export(source_symbol)
                        && let Some(collision) = table.get_mut(&name)
                    {
                        collision.exports_with_duplicate.push(*export_node);
                    }
                }
            }
        }
    }

    /// `resolveSymbol` (`checker.go`): an alias answers its target, with
    /// `unknownSymbol` (here `None`) for one that does not resolve.
    fn resolve_symbol_for_export(&mut self, symbol: SymbolId) -> Option<SymbolId> {
        let symbol = self.binder.merged_symbol(symbol);
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
            self.resolve_alias(symbol).map(|target| self.binder.merged_symbol(target))
        } else {
            Some(symbol)
        }
    }

    /// `scanner.GetTextOfNode(exportNode.ModuleSpecifier())`: the specifier
    /// as written, quotes included.
    fn export_star_specifier_text(&self, specifier: NodeId) -> String {
        let span = self.nodes.span(specifier);
        self.source_file_of_for_diagnostics(specifier)
            .and_then(|file| self.module_host.and_then(|host| host.source_text(file, self.nodes)))
            .and_then(|text| text.get(span.start as usize..span.end as usize))
            .map_or_else(String::new, str::to_string)
    }

    /// `createDiagnosticForNode(node, …)` on the `export *` declaration.
    fn report_export_star_conflict(&mut self, node: NodeId, specifier_text: String, name: String) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span: Span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::MODULE_0_HAS_ALREADY_EXPORTED_A_MEMBER_NAMED_1_CONSIDER_EXPLICITLY_RE_EXPORTING_TO_RESOLVE_THE_AMBIGUITY,
                span,
                [specifier_text, name],
            ),
        );
    }
}
