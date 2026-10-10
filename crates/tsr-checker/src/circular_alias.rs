//! TS2303 — `Circular definition of import alias '{0}'.`
//!
//! `resolveAlias` (`checker.go:16266`-`:16291`). Upstream detects the cycle
//! with `pushTypeResolution(symbol, AliasTarget)` around
//! `getTargetOfAliasDeclaration`. Resolving a target **recurses** into
//! `resolveAlias` whenever the target is itself an alias: `resolveEntityName`,
//! `getExternalModuleMember` and `resolveExternalModuleSymbol` all end in
//! `resolveSymbol`, and `resolveIndirectionAlias` covers a pure-alias target.
//! When the recursion comes back to a symbol already on the stack, every frame
//! from that symbol up pops `false` and reports TS2303 at its own declaration.
//! An alias that only *leads into* a cycle pops `true` and reports nothing.
//!
//! So the set of reporting aliases is exactly the aliases whose target chain
//! returns to themselves. This port's [`Checker::resolve_alias`] does recurse
//! through a pure-alias target (`resolveIndirectionAlias`), but a cycle only
//! completes it as `None` and reports nothing, so the chain is walked here
//! over the uncached one-hop worker
//! ([`Checker::get_target_of_alias_symbol`], `getTargetOfAliasDeclaration`),
//! one hop per recursion upstream would make:
//!
//! - `import x = require("m")`, `import * as x from "m"`, `export * as x from
//!   "m"`: the module's `export=` symbol (`resolveExternalModuleSymbol`).
//! - every other alias form: `getTargetOfAliasDeclaration`'s immediate target.
//!
//! The walk stops at the first non-alias, at an unresolvable hop, or at a
//! repeat that is not the start (a cycle this alias only leads into). This
//! replaces the two syntactic shapes §955/§956 ported
//! (`docs/architecture/checker-notes-diag2.md`), which missed cross-file
//! cycles and `export =` cycles. `docs/parity/notes/names-modules.md` §2.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// Hop bound for the chain walk. A chain is at most one hop per alias in the
/// program; the bound only keeps a defect elsewhere from becoming a hang.
const MAX_ALIAS_HOPS: usize = 64;

impl Checker<'_, '_> {
    /// One alias declaration: report TS2303 when its target chain comes back
    /// to it. Called from every site upstream's `checkAliasSymbol` and
    /// `checkExportAssignment` resolve the alias at.
    pub(crate) fn check_circular_import_alias(&mut self, node: NodeId) {
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS)
            // `getDeclarationOfAliasSymbol` is the declaration upstream
            // reports at; a merged alias reports once, there.
            || self.declaration_of_alias_symbol(symbol) != Some(node)
        {
            return;
        }
        if !self.alias_chain_returns_to(symbol) {
            return;
        }
        let Some(text) = self.alias_name_as_written(node, symbol) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(&messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS_0, span, [text]),
        );
    }

    /// Does resolving `start` recurse back into `start`?
    fn alias_chain_returns_to(&mut self, start: SymbolId) -> bool {
        let mut current = start;
        let mut seen: Vec<SymbolId> = Vec::new();
        for _ in 0..MAX_ALIAS_HOPS {
            let Some(next) = self.alias_recursion_target(current) else { return false };
            let next = self.binder.merged_symbol(next);
            // `resolveSymbol`/`resolveEntityName` recurse only into a
            // **non-local alias** (`IsNonLocalAlias`, `utilities.go`): an alias
            // that does not itself carry a value, type or namespace meaning. An
            // alias merged with a real declaration of its name answers that
            // declaration without recursing — even when it is `start` itself
            // (`export { x }` beside `export let x`, `compiler/multipleExports`).
            let flags = self.binder.symbols().get(next).flags;
            if !flags.intersects(SymbolFlags::ALIAS)
                || flags.intersects(SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE)
            {
                return false;
            }
            if next == start {
                return true;
            }
            if seen.contains(&next) {
                return false;
            }
            seen.push(next);
            current = next;
        }
        false
    }

    /// The symbol upstream's `resolveAlias(alias)` hands to its recursive
    /// `resolveSymbol`/`resolveAlias` call.
    fn alias_recursion_target(&mut self, alias: SymbolId) -> Option<SymbolId> {
        let declaration = self.declaration_of_alias_symbol(alias)?;
        let module_reference = match self.node_map.get(declaration)? {
            Node::ImportEqualsDeclaration(import) => match import.module_reference? {
                tsr_ast::ModuleReference::ExternalModuleReference(reference) => {
                    Some((declaration, reference.expression?.node_id()?))
                }
                // `getSymbolOfPartOfRightHandSideOfImportEquals`: an identifier
                // resolves at `Namespace` meaning, and the hit is handed on
                // unresolved. [`Checker::resolve_alias`]'s identifier arm
                // instead declines an alias hit (or types it), which is the
                // recursion this walk exists to follow.
                tsr_ast::ModuleReference::Identifier(name) => {
                    return self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        name.node_id?,
                        name.text,
                        SymbolFlags::NAMESPACE,
                    );
                }
                tsr_ast::ModuleReference::QualifiedName(_) => None,
            },
            Node::NamespaceImport(_) => {
                let clause = self.nodes.parent(declaration)?;
                let owner = self.nodes.parent(clause)?;
                Some((owner, self.external_module_name(owner)?))
            }
            Node::NamespaceExport(_) => {
                let owner = self.nodes.parent(declaration)?;
                Some((owner, self.external_module_name(owner)?))
            }
            // `getTargetOfExportSpecifier`'s local arm: `resolveEntityName`
            // (`checker.go:14970`), whose `getSymbol` recurses into
            // `resolveAlias` on the first alias the walk meets, before any
            // meaning filter. In a non-ambient namespace that can be the
            // specifier's own alias (`namespace N { export { inner } }`),
            // which is the cycle. `docs/parity/notes/r6-names2.md` §3.
            Node::ExportSpecifier(specifier) => {
                let export = self.nodes.parent(self.nodes.parent(declaration)?)?;
                match (self.node_map.get(export)?, specifier.property_name.or(specifier.name)?) {
                    (
                        Node::ExportDeclaration(export),
                        tsr_ast::ModuleExportName::Identifier(name),
                    ) if export.module_specifier.is_none() => {
                        return self.binder.resolve_name_with_export_alias(
                            self.nodes,
                            self.node_map,
                            name.node_id?,
                            name.text,
                            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
                            |_, _| Some(true),
                        );
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        let Some((owner, specifier)) = module_reference else {
            return self.get_target_of_alias_symbol(alias);
        };
        let module = self.resolve_external_module_name(owner, specifier)?;
        let module = self.binder.merged_symbol(module);
        // `resolveExternalModuleSymbol`: `resolveSymbol(exports["export="])`.
        self.binder.symbols().get(module).exports.get("export=").copied()
    }

    /// `symbolToString` for an alias: `getNameOfSymbolAsWritten` spells an
    /// `export =`/`export default` symbol by its declaration's name, which
    /// for an export assignment is the expression when it is an identifier
    /// (`GetNameOfDeclaration`).
    fn alias_name_as_written(&self, node: NodeId, symbol: SymbolId) -> Option<String> {
        if let Some(Node::ExportAssignment(assignment)) = self.node_map.get(node)
            && let Some(expression) = assignment.expression.and_then(|e| e.node_id())
            && self.nodes.kind(expression) == SyntaxKind::Identifier
        {
            return self.identifier_text(expression).map(str::to_string);
        }
        Some(self.binder.symbols().get(symbol).name.to_string())
    }
}
