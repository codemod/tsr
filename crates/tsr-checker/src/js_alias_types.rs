//! TS18042 / TS18043 — an import or export of a type in a JavaScript file.
//!
//! `checkAliasSymbol` (`checker.go:6751`) runs this arm before its conflict
//! test (TS2440/TS2484): in a JS file, an alias whose target has no value
//! meaning, on a declaration that is not itself type-only, is an error,
//! because a JS file has no type-only imports or exports outside JSDoc:
//!
//! - an export specifier reports TS18043 (`Types cannot appear in export
//!   declarations in JavaScript files.`);
//! - every other alias reports TS18042, naming the written identifier and
//!   the `import("…")` text that would reach it in a JSDoc annotation
//!   (`import("fs").WriteFileOptions` for an import specifier,
//!   `import("fs")` for a default or namespace import). The specifier text
//!   is `TryGetModuleSpecifierFromDeclaration`'s (`nodebuilderimpl.go:1193`)
//!   on the nearest import, import-equals or variable declaration, or `...`.
//!
//! Either way the arm returns, so the conflict test never runs for it. The
//! report is at `node.PropertyNameOrName()`. Its related information
//! (`X_0_is_automatically_exported_here`) is not ported; the suites compare
//! codes and positions.
//!
//! `check_alias_symbol` (`symbols.rs`) recorded this arm as unreachable
//! because allowJs cases were once excluded from the diagnostics suite; they
//! are not any more (`importingExportingTypes`).
//!
//! No cache or side table.
//!
//! `docs/parity/notes/r6-smallcodes5.md` §2.10.

use tsr_ast::{Expression, ModuleExportName, ModuleReference, Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

#[expect(dead_code, reason = "r6-smallcodes5 hook diff not applied")]
impl Checker<'_, '_> {
    /// `checkAliasSymbol`'s JS arm. `true` when it applied, so the caller
    /// returns before its conflict test. `type_only` is
    /// `ast.IsTypeOnlyImportOrExportDeclaration(node)`.
    ///
    /// An alias chain that does not resolve does not report: upstream's
    /// `resolveAlias` ends it at `unknownSymbol`, a `Property`, so its target
    /// flags carry a value meaning and the arm does not run.
    pub(crate) fn report_js_type_alias(
        &mut self,
        node: NodeId,
        local: SymbolId,
        target: SymbolId,
        target_flags: SymbolFlags,
        type_only: bool,
    ) -> bool {
        if !self.in_js_file(node) || target_flags.intersects(SymbolFlags::VALUE) || type_only {
            return false;
        }
        let mut current = target;
        for _ in 0..8 {
            if !self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
                break;
            }
            match self.resolve_alias(current) {
                None => return false,
                // `export { T }` beside an implicitly exported `@typedef T`
                // merges the specifier and the alias into one export symbol,
                // whose target is itself (upstream's related information names
                // it: `alreadyExportedSymbol == target`).
                Some(next) if next == current => break,
                Some(next) => current = next,
            }
        }
        let error_node = self.property_name_or_name(node).unwrap_or(node);
        let Some(file) = self.source_file_of_for_diagnostics(error_node) else { return true };
        let span = self.error_span(error_node);
        if self.nodes.kind(node) == SyntaxKind::ExportSpecifier {
            self.report(
                file,
                Diagnostic::new(
                    &messages::TYPES_CANNOT_APPEAR_IN_EXPORT_DECLARATIONS_IN_JAVASCRIPT_FILES,
                    span,
                ),
            );
            return true;
        }
        let identifier = match self.node_map.get(error_node) {
            Some(Node::Identifier(identifier)) => identifier.text.to_string(),
            _ => self.binder.symbols().get(local).name.to_string(),
        };
        let specifier = self.alias_module_specifier_text(node).unwrap_or_else(|| "...".into());
        let mut import = format!("import(\"{specifier}\")");
        if self.nodes.kind(node) == SyntaxKind::ImportSpecifier {
            import.push('.');
            import.push_str(&identifier);
        }
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_1_IN_A_JSDOC_TYPE_ANNOTATION,
                span,
                [identifier, import],
            ),
        );
        true
    }

    /// `node.PropertyNameOrName()` for an alias declaration.
    fn property_name_or_name(&self, node: NodeId) -> Option<NodeId> {
        let export_name = |name: ModuleExportName<'_>| match name {
            ModuleExportName::Identifier(identifier) => identifier.node_id,
            ModuleExportName::StringLiteral(literal) => literal.node_id,
        };
        match self.node_map.get(node)? {
            Node::ImportSpecifier(specifier) => specifier
                .property_name
                .and_then(export_name)
                .or_else(|| specifier.name.and_then(|name| name.node_id)),
            Node::ExportSpecifier(specifier) => {
                specifier.property_name.or(specifier.name).and_then(export_name)
            }
            Node::ImportClause(clause) => clause.name.and_then(|name| name.node_id),
            Node::NamespaceImport(namespace) => namespace.name.and_then(|name| name.node_id),
            Node::ImportEqualsDeclaration(declaration) => {
                declaration.name.and_then(|name| name.node_id)
            }
            _ => None,
        }
    }

    /// `TryGetModuleSpecifierFromDeclaration` on the nearest import,
    /// import-equals or variable declaration above `node`: the string
    /// literal it imports from, as written.
    fn alias_module_specifier_text(&self, node: NodeId) -> Option<String> {
        let declaration = self.nodes.ancestors(node).find(|&at| {
            matches!(
                self.nodes.kind(at),
                SyntaxKind::ImportDeclaration
                    | SyntaxKind::ImportEqualsDeclaration
                    | SyntaxKind::VariableDeclaration
            )
        })?;
        let specifier = match self.node_map.get(declaration)? {
            Node::ImportDeclaration(import) => import.module_specifier?,
            Node::ImportEqualsDeclaration(import) => match import.module_reference? {
                ModuleReference::ExternalModuleReference(reference) => reference.expression?,
                _ => return None,
            },
            Node::VariableDeclaration(variable) => match variable.initializer? {
                Expression::CallExpression(call)
                    if matches!(call.expression, Some(Expression::Identifier(callee))
                        if callee.text == "require")
                        && call.arguments.len() == 1 =>
                {
                    call.arguments[0]
                }
                _ => return None,
            },
            _ => return None,
        };
        match specifier {
            Expression::StringLiteral(literal) => Some(literal.text.to_string()),
            _ => None,
        }
    }
}
