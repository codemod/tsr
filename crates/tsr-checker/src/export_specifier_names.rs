//! The failure tail of `getTargetOfExportSpecifier`'s local arm.
//!
//! `export { X }` with no module specifier resolves `X` through
//! `resolveEntityName(name, Value|Type|Namespace, ignoreErrors=false)`
//! (`checker.go:14970`), reached from `checkAliasSymbol`'s `resolveAlias`.
//! A name that resolves nowhere runs `onFailedToResolveSymbol`
//! (`checker.go:1564`) with `getCannotFindNameDiagnosticForName`'s message
//! (`checker.go:15784`, `:13915`). The port ran only the
//! exporting-primitive rung of that cascade
//! (`Checker::check_export_specifier_is_local`); this module runs the rest:
//! the missing lib, the spelling suggestion at the specifier's meaning, and
//! the fallback message. `docs/parity/notes/r6-names.md` §9.
//!
//! No cache or side table: one resolution per local export specifier, the same
//! lookup `check_export_specifier_is_local` makes.

use tsr_ast::{Node, NodeId};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

#[allow(dead_code, reason = "hook: docs/parity/notes/r6-names-export-specifier.diff")]
impl Checker<'_, '_> {
    /// `resolveEntityName`'s failure arm for a local `ExportSpecifier`.
    ///
    /// The rungs of `onFailedToResolveSymbol` before the lib arm cannot fire
    /// at this meaning except one: the missing prefix and extending-interface
    /// arms need a class or heritage position, and the namespace/type/value
    /// mismatch arms each look the name up under a meaning
    /// `Value|Type|Namespace` already covers. The remaining rung,
    /// `checkAndReportErrorForExportingPrimitiveType`, is
    /// `report_exporting_primitive_type`, which
    /// `check_export_specifier_is_local` already runs; it is consulted here
    /// only to stay silent after it.
    pub(crate) fn names_check_export_specifier_target(&mut self, node: NodeId) {
        let Some(Node::ExportSpecifier(specifier)) = self.node_map.get(node) else { return };
        // `PropertyNameOrName()`; a string literal resolves to nothing and
        // reports nothing (`checker.go:14967`).
        let named = specifier
            .property_name
            .and_then(|name| name.node_id())
            .or_else(|| specifier.name.and_then(|name| name.node_id()));
        let Some(named) = named else { return };
        let Some(Node::Identifier(identifier)) = self.node_map.get(named) else { return };
        let declaration = self
            .nodes
            .parent(node)
            .and_then(|list| self.nodes.parent(list))
            .and_then(|declaration| self.node_map.get(declaration));
        let Some(Node::ExportDeclaration(export)) = declaration else { return };
        if export.module_specifier.is_some() {
            return;
        }
        // Only a source file's or an ambient module's exports skip a pure
        // export-specifier alias (`binder/nameresolver.go:121-133`). In a
        // non-ambient namespace the walk finds the specifier's own alias
        // (TS2303, a circular alias, not a failed lookup), so nothing here
        // applies.
        if let Some(namespace) = self.nodes.ancestors(node).find(|&ancestor| {
            matches!(self.node_map.get(ancestor), Some(Node::ModuleDeclaration(_)))
        }) && !self.is_in_ambient_context(namespace)
        {
            return;
        }
        let text = identifier.text;
        let span = self.error_span(named);
        // `NodeIsMissing`: the parser's recovered empty name.
        if text.is_empty() || span.start == span.end {
            return;
        }
        // Both are synthesised globals here and resolve natively
        // (`check_export_specifier_is_local` reports TS2661 for them).
        if text == "undefined" || text == "globalThis" {
            return;
        }
        // From the parent scope: the specifier's own alias is not a
        // candidate (`resolve_name_excluding`, §836 of
        // `checker-notes-diag2.md`). `ALIAS` because an import re-exported
        // here is an alias whose target carries the meaning.
        if self
            .binder
            .resolve_name_excluding(
                self.nodes,
                self.node_map,
                named,
                text,
                SymbolFlags::VALUE
                    | SymbolFlags::TYPE
                    | SymbolFlags::NAMESPACE
                    | SymbolFlags::ALIAS,
                Some(node),
            )
            .is_some()
        {
            return;
        }
        if is_primitive_type_name(text) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(named) else { return };
        if let Some(lib) = crate::check::suggested_lib_for(text) {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_1_OR_LATER,
                    span,
                    [text.to_string(), lib.to_string()],
                ),
            );
            return;
        }
        let meaning = SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE;
        if let Some(suggestion) = self.spelling_suggestion_for(named, text, meaning) {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::CANNOT_FIND_NAME_0_DID_YOU_MEAN_1,
                    span,
                    [text.to_string(), suggestion],
                ),
            );
            return;
        }
        let message =
            crate::check::cannot_find_name_message(text).unwrap_or(&messages::CANNOT_FIND_NAME_0);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }
}

/// `isPrimitiveTypeName` (`checker.go:1637`).
fn is_primitive_type_name(name: &str) -> bool {
    matches!(name, "any" | "string" | "number" | "boolean" | "never" | "unknown")
}
