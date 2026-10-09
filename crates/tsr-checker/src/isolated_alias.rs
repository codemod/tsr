//! `checkAliasSymbol`'s single-file-transpilation arms (pinned
//! `checker.go:6788`–`6858`): what `isolatedModules` and
//! `verbatimModuleSyntax` forbid an import or export alias to name.
//!
//! `check_alias_symbol` (`symbols.rs`) ports the head of the function — the
//! resolution and the TS2440/TS2441 conflict arm — and hands the resolved
//! pieces here, so this module never re-resolves the alias. Every rule here
//! reads an option the conflict arm does not, and none of it ran before
//! `tsr-2zk.1108` (`docs/parity/notes/r5-config.md` §6).
//!
//! Related information (`addTypeOnlyDeclarationRelatedInfo`'s "was imported
//! here") is not attached: the diagnostics oracle compares position and code,
//! and this port's `Diagnostic` has no related list at these sites.

use tsr_ast::{ModuleExportName, ModuleReference, Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The arms after the conflict check, for alias declaration `node` whose
    /// local (merged, export-side) symbol is `local` and whose resolved
    /// target is `target` with `target_flags`. `conflicted` is whether the
    /// conflict arm reported, which the TS2865 arm sits in the `else` of.
    pub(crate) fn check_alias_symbol_isolated(
        &mut self,
        node: NodeId,
        local: SymbolId,
        target: SymbolId,
        target_flags: SymbolFlags,
        conflicted: bool,
    ) {
        let kind = self.nodes.kind(node);
        // The `IsInJSFile` arm (`:6750`) returns before any of this when a
        // JavaScript alias names a type and is not itself type-only.
        if self.in_js_file(node)
            && !target_flags.intersects(SymbolFlags::VALUE)
            && !self.is_type_only_import_or_export_declaration(node)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let local_flags = self.binder.symbols().get(local).flags;
        if !conflicted && kind != SyntaxKind::ExportSpecifier {
            // `compilerOptions.IsolatedModules`, deliberately not
            // `GetIsolatedModules()`: `verbatimModuleSyntax` reports the same
            // import below as TS1484.
            let appears_valuey = self.isolated_modules_option
                && !std::iter::once(node)
                    .chain(self.nodes.ancestors(node))
                    .any(|at| self.is_type_only_import_or_export_declaration(at));
            if appears_valuey
                && local_flags.intersects(SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE)
            {
                let name = self.binder.symbols().get(local).name.to_string();
                let span = self.error_span(node);
                self.report(file, Diagnostic::with_args(
                    &messages::IMPORT_0_CONFLICTS_WITH_LOCAL_VALUE_SO_MUST_BE_DECLARED_WITH_A_TYPE_ONLY_IMPORT_WHEN_ISOLATEDMODULES_IS_ENABLED,
                    span,
                    [name, self.isolated_modules_like_flag_name().to_string()],
                ));
            }
        }
        if !self.isolated_modules
            || self.is_type_only_import_or_export_declaration(node)
            || self.declaration_is_in_an_ambient_context(node)
        {
            return;
        }
        let type_only_alias = self.type_only_alias_declaration_node(local);
        let is_type = !target_flags.intersects(SymbolFlags::VALUE);
        if is_type || type_only_alias.is_some() {
            match kind {
                SyntaxKind::ImportClause
                | SyntaxKind::ImportSpecifier
                | SyntaxKind::ImportEqualsDeclaration => {
                    if self.verbatim_module_syntax {
                        let message = if self.is_internal_module_import_equals_declaration(node) {
                            &messages::AN_IMPORT_ALIAS_CANNOT_RESOLVE_TO_A_TYPE_OR_TYPE_ONLY_DECLARATION_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED
                        } else if is_type {
                            &messages::_0_IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED
                        } else {
                            &messages::_0_RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED
                        };
                        let name = self.property_name_or_name_text(node).unwrap_or_default();
                        let span = self.error_span(node);
                        self.report(file, Diagnostic::with_args(message, span, [name]));
                    }
                    if is_type
                        && kind == SyntaxKind::ImportEqualsDeclaration
                        && matches!(self.node_map.get(node), Some(Node::ImportEqualsDeclaration(n))
                            if tsr_ast::has_syntactic_modifier(n.modifiers, SyntaxKind::ExportKeyword))
                    {
                        let span = self.error_span(node);
                        self.report(file, Diagnostic::with_args(
                            &messages::CANNOT_USE_EXPORT_IMPORT_ON_A_TYPE_OR_TYPE_ONLY_NAMESPACE_WHEN_0_IS_ENABLED,
                            span,
                            [self.isolated_modules_like_flag_name().to_string()],
                        ));
                    }
                }
                SyntaxKind::ExportSpecifier => {
                    // `import type { A } from './a'; export { A }` is allowed:
                    // one-file analysis sees the export must be dropped.
                    let other_file = type_only_alias.is_none_or(|declaration| {
                        self.source_file_of(declaration) != self.source_file_of(node)
                    });
                    if self.verbatim_module_syntax || other_file {
                        let name = self.property_name_or_name_text(node).unwrap_or_default();
                        let flag = self.isolated_modules_like_flag_name().to_string();
                        let span = self.error_span(node);
                        let diagnostic = if is_type {
                            Diagnostic::with_args(
                                &messages::RE_EXPORTING_A_TYPE_WHEN_0_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                                span,
                                [flag],
                            )
                        } else {
                            Diagnostic::with_args(
                                &messages::_0_RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE_EXPORT_WHEN_1_IS_ENABLED,
                                span,
                                [name, flag],
                            )
                        };
                        self.report(file, diagnostic);
                    }
                }
                _ => {}
            }
        }
        let is_import_equals = kind == SyntaxKind::ImportEqualsDeclaration;
        let commonjs_file =
            self.alias_emit_module_format_of_file(file) == tsr_core::ModuleKind::CommonJS;
        if self.verbatim_module_syntax
            && !is_import_equals
            && !self.in_js_file(node)
            && commonjs_file
        {
            // `getVerbatimModuleSyntaxErrorMessage` (`checker.go:5681`).
            let commonjs_extension = self
                .module_host
                .and_then(|host| host.file_path(file))
                .is_some_and(|path| tsr_path::file_extension_is_one_of(&path, &[".cts", ".cjs"]));
            let message = if commonjs_extension {
                &messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBATIMMODULESYNTAX
            } else {
                &messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBATIMMODULESYNTAX_ADJUST_THE_TYPE_FIELD_IN_THE_NEAREST_PACKAGE_JSON_TO_MAKE_THIS_FILE_AN_ECMASCRIPT_MODULE_OR_ADJUST_YOUR_VERBATIMMODULESYNTAX_MODULE_AND_MODULERESOLUTION_SETTINGS_IN_TYPESCRIPT
            };
            let span = self.error_span(node);
            self.report(file, Diagnostic::new(message, span));
        } else if self.module_kind == tsr_core::ModuleKind::Preserve
            && !is_import_equals
            && kind != SyntaxKind::VariableDeclaration
            && commonjs_file
        {
            let span = self.error_span(node);
            self.report(file, Diagnostic::new(
                &messages::ECMASCRIPT_MODULE_SYNTAX_IS_NOT_ALLOWED_IN_A_COMMONJS_MODULE_WHEN_MODULE_IS_SET_TO_PRESERVE,
                span,
            ));
        }
        if self.verbatim_module_syntax && target_flags.intersects(SymbolFlags::CONST_ENUM) {
            // `GetProjectReferenceFromOutputDts` has no counterpart here (no
            // project references in a checker's program), so `redirect` is nil.
            let ambient =
                self.binder.symbols().get(target).value_declaration.is_some_and(|declaration| {
                    self.declaration_is_in_an_ambient_context(declaration)
                });
            if ambient {
                let span = self.error_span(node);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_0_IS_ENABLED,
                        span,
                        [self.isolated_modules_like_flag_name().to_string()],
                    ),
                );
            }
        }
    }

    /// `getIsolatedModulesLikeFlagName` (`checker.go`).
    fn isolated_modules_like_flag_name(&self) -> &'static str {
        if self.verbatim_module_syntax { "verbatimModuleSyntax" } else { "isolatedModules" }
    }

    /// `ast.IsTypeOnlyImportOrExportDeclaration`
    /// (`IsTypeOnlyImportDeclaration` || `IsTypeOnlyExportDeclaration`).
    fn is_type_only_import_or_export_declaration(&self, node: NodeId) -> bool {
        let clause_is_type_only = |clause: Option<NodeId>| {
            matches!(clause.and_then(|clause| self.node_map.get(clause)), Some(Node::ImportClause(clause))
                if clause.phase_modifier.is_some_and(|token| token.kind == SyntaxKind::TypeKeyword))
        };
        let parent = |at: NodeId| self.nodes.parent(at);
        match self.node_map.get(node) {
            Some(Node::ImportSpecifier(specifier)) => {
                specifier.is_type_only || clause_is_type_only(parent(node).and_then(parent))
            }
            Some(Node::NamespaceImport(_)) => clause_is_type_only(parent(node)),
            Some(Node::ImportClause(_)) => clause_is_type_only(Some(node)),
            Some(Node::ImportEqualsDeclaration(declaration)) => declaration.is_type_only,
            Some(Node::ExportSpecifier(specifier)) => {
                specifier.is_type_only
                    || matches!(parent(node).and_then(parent).and_then(|at| self.node_map.get(at)),
                        Some(Node::ExportDeclaration(declaration)) if declaration.is_type_only)
            }
            Some(Node::ExportDeclaration(declaration)) => {
                declaration.is_type_only
                    && declaration.module_specifier.is_some()
                    && declaration.export_clause.is_none()
            }
            Some(Node::NamespaceExport(_)) => matches!(
                parent(node).and_then(|at| self.node_map.get(at)),
                Some(Node::ExportDeclaration(declaration)) if declaration.is_type_only
            ),
            _ => false,
        }
    }

    /// `getTypeOnlyAliasDeclaration(symbol)` (`checker.go:1861`) as the
    /// declaration itself: the first alias declaration on the chain that is
    /// type-only, or that reaches its name through a type-only `export *`.
    /// The walk is [`Checker::type_only_alias_declaration`]'s, bounded the
    /// same way; that function answers only *which kind* it found.
    fn type_only_alias_declaration_node(&mut self, symbol: SymbolId) -> Option<NodeId> {
        let mut current = self.binder.merged_symbol(symbol);
        for _ in 0..16 {
            let entry = self.binder.symbols().get(current);
            if !entry.flags.intersects(SymbolFlags::ALIAS) {
                return None;
            }
            let declaration = *entry.declarations.first()?;
            if self.is_type_only_import_or_export_declaration(declaration) {
                return Some(declaration);
            }
            if let Some(star) = self.specifier_type_only_export_star(declaration) {
                return Some(star);
            }
            current = self.binder.merged_symbol(self.resolve_alias(current)?);
        }
        None
    }

    /// `ast.IsInternalModuleImportEqualsDeclaration`: `import x = N.M`.
    fn is_internal_module_import_equals_declaration(&self, node: NodeId) -> bool {
        matches!(self.node_map.get(node), Some(Node::ImportEqualsDeclaration(declaration))
            if !matches!(declaration.module_reference, Some(ModuleReference::ExternalModuleReference(_))))
    }

    /// `node.PropertyNameOrName().Text()` of an alias declaration.
    fn property_name_or_name_text(&self, node: NodeId) -> Option<String> {
        let text = |name: ModuleExportName<'_>| match name {
            ModuleExportName::Identifier(identifier) => identifier.text.to_string(),
            ModuleExportName::StringLiteral(literal) => literal.text.to_string(),
        };
        match self.node_map.get(node)? {
            Node::ImportSpecifier(specifier) => specifier
                .property_name
                .map(text)
                .or_else(|| specifier.name.map(|name| name.text.to_string())),
            Node::ExportSpecifier(specifier) => {
                specifier.property_name.or(specifier.name).map(text)
            }
            Node::ImportClause(clause) => clause.name.map(|name| name.text.to_string()),
            Node::ImportEqualsDeclaration(declaration) => {
                declaration.name.map(|name| name.text.to_string())
            }
            Node::NamespaceImport(import) => import.name.map(|name| name.text.to_string()),
            _ => None,
        }
    }

    /// `program.GetEmitModuleFormatOfFile` (`program.go`): the file's implied
    /// format for emit, else the emit module kind.
    fn alias_emit_module_format_of_file(&self, file: NodeId) -> tsr_core::ModuleKind {
        match self.module_host.map(|host| host.implied_node_format_for_emit(file)) {
            Some(format) if format != tsr_core::ModuleKind::None => format,
            _ => self.module_kind,
        }
    }
}
