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

use tsr_ast::{ModuleExportName, ModuleReference, Node, NodeId, SyntaxKind, TypeNode};
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
            let message = self.verbatim_module_syntax_error_message(file);
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

    /// `checkExportAssignment`'s single-file-transpilation arms (pinned
    /// `checker.go:5609`–`5650`): what an `export =` / `export default` of a
    /// bare identifier may name under `verbatimModuleSyntax` (TS1282–TS1285)
    /// and `isolatedModules` (TS1289–TS1292), and the `export default` of a
    /// CommonJS-format file under `verbatimModuleSyntax` (TS1286/TS1295 via
    /// `getVerbatimModuleSyntaxErrorMessage`).
    ///
    /// `ambient` is the walk's `NodeFlagsAmbient`. The caller
    /// (`check.rs::check_export_assignment_alone`) runs this after the
    /// TS1120 modifier report, which is where upstream reads it.
    /// `markLinkedReferences(node, ReferenceHintExportAssignment)` is not
    /// ported here: it only publishes the alias-referenced mark, which no
    /// report in this arm reads.
    #[allow(
        dead_code,
        reason = "called by the held hook docs/parity/notes/r6-isolated-export-assignment.diff"
    )]
    pub(crate) fn check_export_assignment_isolated(&mut self, node: NodeId, ambient: bool) {
        let Some(Node::ExportAssignment(assignment)) = self.node_map.get(node) else { return };
        let is_export_equals = assignment.is_export_equals;
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let illegal_export_default_in_cjs = !is_export_equals
            && !ambient
            && self.verbatim_module_syntax
            && self.alias_emit_module_format_of_file(file) == tsr_core::ModuleKind::CommonJS;
        if let Some(tsr_ast::Expression::Identifier(identifier)) = assignment.expression
            && let Some(id) = identifier.node_id
        {
            let text = identifier.text.to_string();
            // `getExportSymbolOfValueSymbolIfExported(resolveEntityName(id,
            // All, ignoreErrors, dontResolveAlias, node))`.
            let symbol = self
                .resolve_name_with_export_alias(id, &text, SymbolFlags::all())
                .map(|symbol| self.export_symbol_of_value_symbol_if_exported(symbol));
            if let Some(symbol) = symbol {
                let symbol_flags = self.binder.symbols().get(symbol).flags;
                let type_only_declaration =
                    self.type_only_alias_declaration_node_ex(symbol, SymbolFlags::VALUE);
                let type_only_in_other_file = type_only_declaration.is_some_and(|declaration| {
                    self.source_file_of(declaration) != self.source_file_of(node)
                });
                let checks = !illegal_export_default_in_cjs && !ambient;
                let span = self.error_span(id);
                if self.get_symbol_flags(symbol).intersects(SymbolFlags::VALUE) {
                    if checks && self.verbatim_module_syntax && type_only_declaration.is_some() {
                        let message = if is_export_equals {
                            &messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BUT_0_RESOLVES_TO_A_TYPE_ONLY_DECLARATION
                        } else {
                            &messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BUT_0_RESOLVES_TO_A_TYPE_ONLY_DECLARATION
                        };
                        self.report(file, Diagnostic::with_args(message, span, [text.clone()]));
                    }
                } else if checks && self.verbatim_module_syntax {
                    let message = if is_export_equals {
                        &messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BUT_0_ONLY_REFERS_TO_A_TYPE
                    } else {
                        &messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BUT_0_ONLY_REFERS_TO_A_TYPE
                    };
                    self.report(file, Diagnostic::with_args(message, span, [text.clone()]));
                }
                if checks && self.isolated_modules && !symbol_flags.intersects(SymbolFlags::VALUE) {
                    let non_local_meanings = self.symbol_flags_ex(symbol, false, true);
                    let flag = self.isolated_modules_like_flag_name().to_string();
                    if symbol_flags.intersects(SymbolFlags::ALIAS)
                        && non_local_meanings.intersects(SymbolFlags::TYPE)
                        && !non_local_meanings.intersects(SymbolFlags::VALUE)
                        && (type_only_declaration.is_none() || type_only_in_other_file)
                    {
                        let message = if is_export_equals {
                            &messages::_0_RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_WHEN_1_IS_ENABLED_CONSIDER_USING_IMPORT_TYPE_WHERE_0_IS_IMPORTED
                        } else {
                            &messages::_0_RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_WHEN_1_IS_ENABLED_CONSIDER_USING_EXPORT_TYPE_0_AS_DEFAULT
                        };
                        self.report(file, Diagnostic::with_args(message, span, [text, flag]));
                    } else if type_only_in_other_file {
                        let message = if is_export_equals {
                            &messages::_0_RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_WHEN_1_IS_ENABLED_CONSIDER_USING_IMPORT_TYPE_WHERE_0_IS_IMPORTED
                        } else {
                            &messages::_0_RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_WHEN_1_IS_ENABLED_CONSIDER_USING_EXPORT_TYPE_0_AS_DEFAULT
                        };
                        self.report(file, Diagnostic::with_args(message, span, [text, flag]));
                    }
                }
            }
        }
        if illegal_export_default_in_cjs {
            let message = self.verbatim_module_syntax_error_message(file);
            let span = self.error_span(node);
            self.report(file, Diagnostic::new(message, span));
        }
    }

    /// TS2866 — `resolveNameHelper`'s success tail (pinned
    /// `checker.go:1872`–`1885`): a value reference in a module that resolved
    /// to a **global** while the file's own top level holds a non-value
    /// meaning of the same name through a non-type-only import. A transpiler
    /// that sees only this file would bind the reference to the import.
    ///
    /// `node` is the reference (upstream's `errorLocation`), `result` the
    /// symbol `resolveName` returned for `text` at a meaning that holds every
    /// `Value` bit. Reads `compilerOptions.IsolatedModules` itself, not
    /// `GetIsolatedModules()`: `verbatimModuleSyntax` already reports the
    /// import as TS1484. `lastLocation` is the reference's source file
    /// whenever the result is a global: the walk reaches `c.globals` only
    /// after leaving the file.
    #[allow(
        dead_code,
        reason = "called by the held hook docs/parity/notes/r6-isolated-global-value.diff"
    )]
    pub(crate) fn check_import_conflicts_with_global_value(
        &mut self,
        node: NodeId,
        result: SymbolId,
        text: &str,
    ) {
        if !self.isolated_modules_option {
            return;
        }
        let Some(file) = self.source_file_of(node) else { return };
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return };
        if !tsr_binder::is_external_module_in(source, self.nodes) {
            return;
        }
        let meaning = SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE;
        let global = self.binder.globals().get(text).copied();
        if global.and_then(|global| self.get_symbol_in(global, meaning))
            != Some(self.binder.merged_symbol(result))
        {
            return;
        }
        let Some(local) = self.binder.locals(file).and_then(|locals| locals.get(text)).copied()
        else {
            return;
        };
        if self.get_symbol_in(local, !SymbolFlags::VALUE).is_none() {
            return;
        }
        let local = self.binder.merged_symbol(local);
        let import =
            self.binder.symbols().get(local).declarations.iter().copied().find(|&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::ImportSpecifier
                        | SyntaxKind::ImportClause
                        | SyntaxKind::NamespaceImport
                        | SyntaxKind::ImportEqualsDeclaration
                )
            });
        let Some(import) = import else { return };
        if self.is_type_only_import_or_export_declaration(import) {
            return;
        }
        let Some(report_file) = self.source_file_of_for_diagnostics(import) else { return };
        let span = self.error_span(import);
        self.report(report_file, Diagnostic::with_args(
            &messages::IMPORT_0_CONFLICTS_WITH_GLOBAL_VALUE_USED_IN_THIS_FILE_SO_MUST_BE_DECLARED_WITH_A_TYPE_ONLY_IMPORT_WHEN_ISOLATEDMODULES_IS_ENABLED,
            span,
            [text.to_string()],
        ));
    }

    /// `getSymbol(symbols, name, meaning)` (`checker.go:2176`) for the entry
    /// `symbol` already looked up by name: the merged symbol when it carries
    /// `meaning`, or is an alias whose chain does.
    fn get_symbol_in(&mut self, symbol: SymbolId, meaning: SymbolFlags) -> Option<SymbolId> {
        let symbol = self.binder.merged_symbol(symbol);
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(meaning) {
            return Some(symbol);
        }
        (flags.intersects(SymbolFlags::ALIAS) && self.get_symbol_flags(symbol).intersects(meaning))
            .then_some(symbol)
    }

    /// `checkConstEnumAccess` (pinned `checker.go:7573`–`7597`), which
    /// `checkExpressionEx` runs on every expression whose type is a const
    /// enum's object type: TS2475 when the expression is not a property or
    /// element access receiver, an import/export-assignment entity name, a
    /// type query or an export specifier; TS2748 when the enum is ambient and
    /// `isolatedModules` is set, or `verbatimModuleSyntax` is set and the
    /// name does not reach the enum through an import (an import of it is
    /// `checkAliasSymbol`'s TS2748 instead).
    ///
    /// `t` must be a const enum object type
    /// ([`Checker::is_const_enum_object_type`]). `GetProjectReferenceFromOutputDts`
    /// has no counterpart (no project references in a checker's program), so
    /// `redirect` is nil, as in [`Checker::check_alias_symbol_isolated`].
    #[allow(
        dead_code,
        reason = "called by the held hook docs/parity/notes/r6-isolated-const-enum-access.diff"
    )]
    pub(crate) fn check_const_enum_access(&mut self, node: NodeId, t: crate::types::TypeId) {
        let crate::types::TypeData::Anonymous { symbol, .. } = self.store.get(t).data else {
            return;
        };
        let Some(parent) = self.nodes.parent(node) else { return };
        let is_child = |expression: Option<tsr_ast::Expression<'_>>| {
            expression.and_then(|expression| expression.node_id()) == Some(node)
        };
        let kind = self.nodes.kind(node);
        let ok = match self.node_map.get(parent) {
            Some(Node::PropertyAccessExpression(access)) if is_child(access.expression) => true,
            Some(Node::ElementAccessExpression(access)) if is_child(access.expression) => true,
            Some(Node::ExportSpecifier(_)) => true,
            _ => {
                matches!(kind, SyntaxKind::Identifier | SyntaxKind::QualifiedName)
                    && (self.is_in_right_side_of_import_or_export_assignment(node)
                        || self.nodes.kind(parent) == SyntaxKind::TypeQuery)
            }
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if !ok {
            let span = self.error_span(node);
            self.report(file, Diagnostic::new(
                &messages::CONST_ENUMS_CAN_ONLY_BE_USED_IN_PROPERTY_OR_INDEX_ACCESS_EXPRESSIONS_OR_THE_RIGHT_HAND_SIDE_OF_AN_IMPORT_DECLARATION_OR_EXPORT_ASSIGNMENT_OR_TYPE_QUERY,
                span,
            ));
        }
        let checks = self.isolated_modules_option
            || (self.verbatim_module_syntax
                && ok
                && self.first_identifier_of(node).is_none_or(|(identifier, text)| {
                    self.binder
                        .resolve_name(
                            self.nodes,
                            self.node_map,
                            identifier,
                            text,
                            SymbolFlags::ALIAS,
                        )
                        .is_none()
                }));
        if !checks {
            return;
        }
        let ambient = self
            .binder
            .symbols()
            .get(symbol)
            .value_declaration
            .is_some_and(|declaration| self.declaration_is_in_an_ambient_context(declaration));
        if ambient && !self.is_valid_type_only_alias_use_site_native(node) {
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

    /// `isInRightSideOfImportOrExportAssignment` (`utilities.go:1107`).
    fn is_in_right_side_of_import_or_export_assignment(&self, mut node: NodeId) -> bool {
        while let Some(parent) = self.nodes.parent(node)
            && self.nodes.kind(parent) == SyntaxKind::QualifiedName
        {
            node = parent;
        }
        match self.nodes.parent(node).and_then(|parent| self.node_map.get(parent)) {
            Some(Node::ImportEqualsDeclaration(declaration)) => {
                declaration.module_reference.and_then(|reference| reference.node_id()) == Some(node)
            }
            Some(Node::ExportAssignment(assignment)) => {
                assignment.expression.and_then(|expression| expression.node_id()) == Some(node)
            }
            _ => false,
        }
    }

    /// `ast.GetFirstIdentifier` over an entity name or entity-name
    /// expression, with its text.
    fn first_identifier_of(&self, mut node: NodeId) -> Option<(NodeId, &str)> {
        loop {
            match self.node_map.get(node)? {
                Node::Identifier(identifier) => return Some((node, identifier.text)),
                Node::QualifiedName(name) => node = name.left?.node_id()?,
                Node::PropertyAccessExpression(access) => node = access.expression?.node_id()?,
                _ => return None,
            }
        }
    }

    /// `ast.IsValidTypeOnlyAliasUseSite` (`ast/utilities.go:3124`) as
    /// upstream writes it. `check.rs`'s `is_valid_type_only_alias_use_site`
    /// is TS1361's tuned copy (it also admits an `export =` operand, which
    /// upstream's `!IsExpressionNode` disjunct does not), so the TS2748 use
    /// site asks this one. `NodeFlagsAmbient` is the ancestor walk this parser
    /// needs (it never sets the flag); `NodeFlagsJSDoc` is read as set.
    fn is_valid_type_only_alias_use_site_native(&self, node: NodeId) -> bool {
        if self.nodes.flags(node).contains(tsr_ast::NodeFlags::JSDOC)
            || self.declaration_is_in_an_ambient_context(node)
        {
            return true;
        }
        // `IsPartOfTypeQuery`.
        let mut at = node;
        while matches!(self.nodes.kind(at), SyntaxKind::QualifiedName | SyntaxKind::Identifier) {
            let Some(parent) = self.nodes.parent(at) else { break };
            at = parent;
        }
        if self.nodes.kind(at) == SyntaxKind::TypeQuery
            || self.identifier_in_non_emitting_heritage_clause(node)
        {
            return true;
        }
        // `isPartOfPossiblyValidTypeOrAbstractComputedPropertyName`.
        let mut at = node;
        while matches!(
            self.nodes.kind(at),
            SyntaxKind::Identifier | SyntaxKind::PropertyAccessExpression
        ) {
            let Some(parent) = self.nodes.parent(at) else { break };
            at = parent;
        }
        if self.nodes.kind(at) == SyntaxKind::ComputedPropertyName
            && let Some(member) = self.nodes.parent(at)
        {
            let abstract_member =
                self.node_map.get(member).and_then(crate::check::modifiers_of).is_some_and(
                    |modifiers| {
                        tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::AbstractKeyword)
                    },
                );
            if abstract_member
                || self.nodes.parent(member).is_some_and(|owner| {
                    matches!(
                        self.nodes.kind(owner),
                        SyntaxKind::InterfaceDeclaration | SyntaxKind::TypeLiteral
                    )
                })
            {
                return true;
            }
        }
        let tree = tsr_ast::Tree { nodes: self.nodes, map: self.node_map };
        let shorthand_name = matches!(
            self.nodes.parent(node).and_then(|parent| self.node_map.get(parent)),
            Some(Node::ShorthandPropertyAssignment(assignment))
                if assignment.name.node_id() == Some(node)
        );
        !(tsr_ast::predicates::is_expression_node(node, tree) || shorthand_name)
    }

    /// `checkDecorators`' `markLinkedReferences(node, ReferenceHintDecorator)`
    /// (pinned `checker.go:6053`, `:28186`, `:28211`) and the function it
    /// dispatches to, `markDecoratorAliasReferenced` (`:28686`): for a
    /// decorated declaration under `emitDecoratorMetadata`, the type names
    /// whose runtime value the metadata will serialize. TS1272 is what this
    /// reports; everything else upstream does here only marks aliases
    /// referenced (`docs/parity/notes/r6-isolated.md` §3 on why that side
    /// table is not built).
    ///
    /// `emit_decorator_metadata` is `compilerOptions.EmitDecoratorMetadata`;
    /// the caller passes it because `Checker` keeps no copy of the option
    /// until the held hook adds one.
    #[allow(
        dead_code,
        reason = "called by the held hook docs/parity/notes/r6-isolated-decorator-metadata.diff"
    )]
    pub(crate) fn check_decorator_linked_references(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
        emit_decorator_metadata: bool,
    ) {
        // Everything upstream evaluates before `markDecoratorAliasReferenced`'s
        // `EmitDecoratorMetadata` test is a pure guard, so the two option
        // reads go first: this runs on every node the walk visits.
        if !emit_decorator_metadata || self.verbatim_module_syntax {
            return;
        }
        // `checkDecorators`' entry guard (`checker.go:6024`).
        let Some(modifiers) = crate::check::modifiers_of(typed) else { return };
        let Some(first_decorator) = modifiers.iter().find_map(|modifier| match modifier {
            tsr_ast::ModifierLike::Decorator(decorator) => decorator.node_id,
            tsr_ast::ModifierLike::Token(_) => None,
        }) else {
            return;
        };
        if !self.decorated_node_can_be_decorated(node, typed) {
            return;
        }
        // `markLinkedReferences`' own guards: `canCollectSymbolAliasAccessibilityData`
        // (`!VerbatimModuleSyntax`, `checker.go:928`, tested above) and the
        // ambient return, which spares a property (decorated or not).
        if !matches!(typed, Node::PropertyDeclaration(_))
            && self.declaration_is_in_an_ambient_context(node)
        {
            return;
        }
        // `markDecoratorAliasReferenced` (its option test is above).
        self.check_external_emit_helpers(first_decorator, crate::emit_helpers::helpers::METADATA);
        match typed {
            Node::ClassDeclaration(class) => {
                // `ast.GetFirstConstructorWithBody`.
                let constructor = class.members.iter().find_map(|member| match member {
                    tsr_ast::ClassElement::ConstructorDeclaration(constructor)
                        if constructor.body.is_some() =>
                    {
                        Some(*constructor)
                    }
                    _ => None,
                });
                if let Some(constructor) = constructor {
                    for parameter in constructor.parameters {
                        self.mark_decorator_metadata_type_node_as_referenced(
                            parameter_type_node_for_decorator_check(parameter),
                        );
                    }
                }
            }
            Node::GetAccessorDeclaration(_) | Node::SetAccessorDeclaration(_) => {
                let other_kind = if matches!(typed, Node::SetAccessorDeclaration(_)) {
                    SyntaxKind::GetAccessor
                } else {
                    SyntaxKind::SetAccessor
                };
                // `ast.GetDeclarationOfKind(c.getSymbolOfDeclaration(node), otherKind)`.
                let other = self.binder.symbol_of(node).and_then(|symbol| {
                    let symbol = self.binder.merged_symbol(symbol);
                    self.binder
                        .symbols()
                        .get(symbol)
                        .declarations
                        .iter()
                        .copied()
                        .find(|&declaration| self.nodes.kind(declaration) == other_kind)
                });
                let annotation = annotated_accessor_type_node(typed).or_else(|| {
                    other
                        .and_then(|other| self.node_map.get(other))
                        .and_then(annotated_accessor_type_node)
                });
                self.mark_decorator_metadata_type_node_as_referenced(annotation);
            }
            Node::MethodDeclaration(method) => {
                for parameter in method.parameters {
                    self.mark_decorator_metadata_type_node_as_referenced(
                        parameter_type_node_for_decorator_check(parameter),
                    );
                }
                self.mark_decorator_metadata_type_node_as_referenced(method.r#type);
            }
            Node::PropertyDeclaration(property) => {
                self.mark_decorator_metadata_type_node_as_referenced(property.r#type);
            }
            Node::ParameterDeclaration(parameter) => {
                self.mark_decorator_metadata_type_node_as_referenced(
                    parameter_type_node_for_decorator_check(parameter),
                );
                let signature =
                    self.nodes.parent(node).and_then(|parent| self.node_map.get(parent));
                let (parameters, return_type) = match signature {
                    Some(Node::MethodDeclaration(n)) => (n.parameters, n.r#type),
                    Some(Node::ConstructorDeclaration(n)) => (n.parameters, n.r#type),
                    Some(Node::SetAccessorDeclaration(n)) => (n.parameters, n.r#type),
                    Some(Node::GetAccessorDeclaration(n)) => (n.parameters, n.r#type),
                    Some(Node::FunctionDeclaration(n)) => (n.parameters, n.r#type),
                    _ => return,
                };
                for parameter in parameters {
                    self.mark_decorator_metadata_type_node_as_referenced(
                        parameter_type_node_for_decorator_check(parameter),
                    );
                }
                self.mark_decorator_metadata_type_node_as_referenced(return_type);
            }
            _ => {}
        }
    }

    /// `markDecoratorMedataDataTypeNodeAsReferenced` (`checker.go:28742`).
    fn mark_decorator_metadata_type_node_as_referenced(&mut self, node: Option<TypeNode<'_>>) {
        if let Some(entity_name) = self.entity_name_for_decorator_metadata(node) {
            self.mark_entity_name_or_entity_expression_as_reference(entity_name, true);
        }
    }

    /// `getEntityNameForDecoratorMetadata` (`checker.go:28749`). Its only
    /// non-nil answer is a type reference's `TypeName`, which is always an
    /// entity name, so the caller's `ast.IsEntityName` test is the type.
    fn entity_name_for_decorator_metadata<'n>(
        &self,
        node: Option<TypeNode<'n>>,
    ) -> Option<tsr_ast::EntityName<'n>> {
        match node? {
            TypeNode::IntersectionTypeNode(n) => self
                .entity_name_for_decorator_metadata_from_type_list(
                    n.types.iter().copied().map(Some),
                ),
            TypeNode::UnionTypeNode(n) => self.entity_name_for_decorator_metadata_from_type_list(
                n.types.iter().copied().map(Some),
            ),
            TypeNode::ConditionalTypeNode(n) => self
                .entity_name_for_decorator_metadata_from_type_list(
                    [n.true_type, n.false_type].into_iter(),
                ),
            TypeNode::ParenthesizedTypeNode(n) => self.entity_name_for_decorator_metadata(n.r#type),
            TypeNode::NamedTupleMember(n) => self.entity_name_for_decorator_metadata(n.r#type),
            TypeNode::TypeReferenceNode(n) => n.type_name,
            _ => None,
        }
    }

    /// `getEntityNameForDecoratorMetadataFromTypeList` (`checker.go:28770`).
    /// A missing list element (a parse hole) is upstream's nil node, which
    /// its `Kind` reads would not survive; it answers "serialized as
    /// `Object`", the nil result.
    fn entity_name_for_decorator_metadata_from_type_list<'n>(
        &self,
        type_nodes: impl Iterator<Item = Option<TypeNode<'n>>>,
    ) -> Option<tsr_ast::EntityName<'n>> {
        let mut common: Option<tsr_ast::EntityName<'n>> = None;
        for type_node in type_nodes {
            let type_node = type_node?;
            if let TypeNode::KeywordTypeNode(keyword) = type_node {
                if keyword.kind == SyntaxKind::NeverKeyword {
                    continue;
                }
                if !self.strict_null_checks && keyword.kind == SyntaxKind::UndefinedKeyword {
                    continue;
                }
            }
            if !self.strict_null_checks
                && let TypeNode::LiteralTypeNode(literal) = type_node
                && literal
                    .literal
                    .and_then(|literal| literal.node_id())
                    .is_some_and(|id| self.nodes.kind(id) == SyntaxKind::NullKeyword)
            {
                continue;
            }
            let individual = self.entity_name_for_decorator_metadata(Some(type_node))?;
            match common {
                None => common = Some(individual),
                Some(tsr_ast::EntityName::Identifier(left)) => match individual {
                    tsr_ast::EntityName::Identifier(right) if left.text == right.text => {}
                    _ => return None,
                },
                Some(tsr_ast::EntityName::QualifiedName(_)) => return None,
            }
        }
        common
    }

    /// `markEntityNameOrEntityExpressionAsReference` (`checker.go:28857`).
    ///
    /// The first arm is `markAliasSymbolAsReferenced`, whose only effect is
    /// `aliasSymbolLinks.referenced` (§3): it is evaluated for its control
    /// flow (TS1272 is its `else`) and publishes nothing.
    fn mark_entity_name_or_entity_expression_as_reference(
        &mut self,
        type_name: tsr_ast::EntityName<'_>,
        for_decorator_metadata: bool,
    ) {
        let Some(type_name_id) = type_name.node_id() else { return };
        let Some((root_name, text)) = self.first_identifier_of(type_name_id) else { return };
        let meaning = if matches!(type_name, tsr_ast::EntityName::Identifier(_)) {
            SymbolFlags::TYPE
        } else {
            SymbolFlags::NAMESPACE
        } | SymbolFlags::ALIAS;
        let text = text.to_string();
        let Some(root_symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, root_name, &text, meaning)
        else {
            return;
        };
        let root_symbol = self.binder.merged_symbol(root_symbol);
        if !self.binder.symbols().get(root_symbol).flags.intersects(SymbolFlags::ALIAS) {
            return;
        }
        let is_value = self.symbol_is_value(root_symbol);
        let can_collect = !self.verbatim_module_syntax;
        let marks = can_collect
            && is_value
            && !self
                .resolve_alias(root_symbol)
                .is_some_and(|target| self.is_const_enum_or_const_enum_only_module(target))
            && self.type_only_alias_declaration_node(root_symbol).is_none();
        if marks {
            // `markAliasSymbolAsReferenced(rootSymbol)`: no side table (§3).
            return;
        }
        let declarations_type_only = self.binder.symbols().get(root_symbol).declarations.clone();
        if for_decorator_metadata
            && self.isolated_modules
            && self.module_kind >= tsr_core::ModuleKind::ES2015
            && !is_value
            && !declarations_type_only
                .iter()
                .any(|&declaration| self.is_type_only_import_or_export_declaration(declaration))
        {
            let Some(file) = self.source_file_of_for_diagnostics(type_name_id) else { return };
            let span = self.error_span(type_name_id);
            self.report(file, Diagnostic::new(
                &messages::A_TYPE_REFERENCED_IN_A_DECORATED_SIGNATURE_MUST_BE_IMPORTED_WITH_IMPORT_TYPE_OR_A_NAMESPACE_IMPORT_WHEN_ISOLATEDMODULES_AND_EMITDECORATORMETADATA_ARE_ENABLED,
                span,
            ));
        }
    }

    /// `isConstEnumOrConstEnumOnlyModule` (`emitresolver.go:689`). This
    /// binder has no `SymbolFlagsConstEnumOnlyModule`, so a namespace holding
    /// only const enums answers `false`; the answer only gates the mark (§3).
    fn is_const_enum_or_const_enum_only_module(&self, symbol: SymbolId) -> bool {
        let flags = self.binder.symbols().get(self.binder.merged_symbol(symbol)).flags;
        flags.intersects(SymbolFlags::CONST_ENUM)
    }

    /// `ast.NodeCanBeDecorated(c.legacyDecorators, node, node.Parent,
    /// node.Parent.Parent)` (`ast/utilities.go:4254`), with `CanHaveDecorators`
    /// folded in as the kinds it lists. `grammar.rs` has a port of the same
    /// predicate, private to that file.
    fn decorated_node_can_be_decorated(&self, node: NodeId, typed: Node<'_>) -> bool {
        let legacy = self.legacy_decorators;
        let parent = self.nodes.parent(node);
        let parent_kind = parent.map(|parent| self.nodes.kind(parent));
        let parent_is_class_declaration = parent_kind == Some(SyntaxKind::ClassDeclaration);
        let parent_is_class_like =
            matches!(parent_kind, Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression));
        let private_name = |name: tsr_ast::PropertyName<'_>| {
            matches!(name, tsr_ast::PropertyName::PrivateIdentifier(_))
        };
        let class_member = |body: bool| {
            body && (if legacy { parent_is_class_declaration } else { parent_is_class_like })
        };
        match typed {
            Node::ClassDeclaration(_) => true,
            Node::ClassExpression(_) => !legacy,
            Node::PropertyDeclaration(property) => {
                !(legacy && private_name(property.name))
                    && ((legacy && parent_is_class_declaration)
                        || (!legacy
                            && parent_is_class_like
                            && !tsr_ast::has_syntactic_modifier(
                                property.modifiers,
                                SyntaxKind::AbstractKeyword,
                            )
                            && !tsr_ast::has_syntactic_modifier(
                                property.modifiers,
                                SyntaxKind::DeclareKeyword,
                            )))
            }
            Node::MethodDeclaration(method) => {
                !(legacy && private_name(method.name)) && class_member(method.body.is_some())
            }
            Node::GetAccessorDeclaration(accessor) => {
                !(legacy && private_name(accessor.name)) && class_member(accessor.body.is_some())
            }
            Node::SetAccessorDeclaration(accessor) => {
                !(legacy && private_name(accessor.name)) && class_member(accessor.body.is_some())
            }
            Node::ParameterDeclaration(_) => {
                if !legacy {
                    return false;
                }
                let Some(parent) = parent else { return false };
                let (body, parameters) = match self.node_map.get(parent) {
                    Some(Node::ConstructorDeclaration(n)) => (n.body.is_some(), n.parameters),
                    Some(Node::MethodDeclaration(n)) => (n.body.is_some(), n.parameters),
                    Some(Node::SetAccessorDeclaration(n)) => (n.body.is_some(), n.parameters),
                    _ => return false,
                };
                // `GetThisParameter(parent) != node`.
                let this_parameter = parameters.first().filter(|first| {
                    matches!(first.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
                });
                body && this_parameter.and_then(|first| first.node_id) != Some(node)
                    && self.nodes.parent(parent).is_some_and(|grandparent| {
                        self.nodes.kind(grandparent) == SyntaxKind::ClassDeclaration
                    })
            }
            _ => false,
        }
    }

    /// `resolveExternalModule`'s resolution-diagnostic branch (pinned
    /// `checker.go:15209`–`15260`, `:15388`), for an import whose specifier
    /// the program resolved to a file: `module.GetResolutionDiagnostic`
    /// (`module/util.go:123`) over the resolver's extension, and the
    /// `ResolvedUsingTsExtension` arms (TS2846, TS5097).
    ///
    /// Returns whether upstream's `sourceFile` is nil — a resolution
    /// diagnostic other than the JSX one keeps the file out — in which case
    /// this reported upstream's not-found answer and the caller's
    /// found-file reports (TS7016, TS2306, the mode family) must not run.
    /// The JavaScript arm (TS7016) is the caller's
    /// (`check.rs::check_untyped_module_import`) and is left to it.
    ///
    /// `resolved` is what the host's `ResolvedModule` carries; `None` when
    /// the program did not resolve the specifier or the host cannot say.
    #[allow(
        dead_code,
        reason = "called by the held hook docs/parity/notes/r6-isolated-resolution-diagnostic.diff"
    )]
    pub(crate) fn check_module_resolution_diagnostic(
        &mut self,
        declaration: NodeId,
        specifier: NodeId,
        resolved: Option<ResolvedModuleFacts<'_>>,
        options: ResolutionDiagnosticOptions,
    ) -> bool {
        let Some(resolved) = resolved else { return false };
        let Some(importing) = self.source_file_of_for_diagnostics(specifier) else { return false };
        let Some(host) = self.module_host else { return false };
        let diagnostic = resolution_diagnostic(
            resolved.extension,
            || host.is_declaration_file(importing),
            self.jsx_emit == tsr_core::JsxEmit::None,
            self.no_implicit_any,
            options,
        );
        // Neither branch reports without a resolution diagnostic or a TS
        // extension in the specifier: the common import stops here, before
        // the string work below.
        if diagnostic.is_none() && !resolved.resolved_using_ts_extension {
            return false;
        }
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else { return false };
        let module_reference = literal.text;
        // `tryFindAmbientModule` (`checker.go:15154`) answers first.
        if !tsr_path::is_external_module_name_relative(module_reference)
            && self.binder.globals().get(format!("\"{module_reference}\"").as_str()).is_some()
        {
            return false;
        }
        // The usage location's mode, asked only now: the facts came from the
        // resolution every mode of this file agrees on whenever there is one.
        let mode = self.module_resolution_mode(host, importing, declaration);
        let resolved_file_name = host
            .resolved_module_path_in_mode(importing, module_reference, mode)
            .unwrap_or_default();
        let jsx = diagnostic.is_none_or(|message| {
            message.code() == messages::MODULE_0_WAS_RESOLVED_TO_1_BUT_JSX_IS_NOT_SET.code()
        });
        let span = self.error_span(specifier);
        if resolved.in_program && jsx {
            // `if sourceFile != nil`: the resolution diagnostic is reported even
            // though the file is used. Without a module symbol the caller's
            // untyped-module arm reports the same TS6142, so only the typed
            // case is reported here.
            if let Some(message) = diagnostic
                && self.resolve_external_module_name(declaration, specifier).is_some()
            {
                self.report(
                    importing,
                    Diagnostic::with_args(
                        message,
                        span,
                        [module_reference.to_string(), resolved_file_name.clone()],
                    ),
                );
            }
            let emittable = std::iter::once(specifier)
                .chain(self.nodes.ancestors(specifier))
                .any(|at| self.is_emittable_import(at));
            if resolved.resolved_using_ts_extension
                && tsr_path::is_declaration_file_name(module_reference)
            {
                if emittable {
                    let ts_extension = tsr_path::try_extract_ts_extension(module_reference);
                    let suggestion =
                        self.suggested_import_source(module_reference, ts_extension, mode, options);
                    self.report(importing, Diagnostic::with_args(
                        &messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT_AN_IMPLEMENTATION_FILE_0_INSTEAD,
                        span,
                        [suggestion],
                    ));
                }
            } else if resolved.resolved_using_ts_extension
                && !self.allow_importing_ts_extensions_from(importing, options)
                && emittable
            {
                let mut ts_extension = tsr_path::try_extract_ts_extension(module_reference);
                if ts_extension.is_empty() {
                    ts_extension = tsr_path::extension::SUPPORTED_TS_EXTENSIONS_FLAT
                        .iter()
                        .copied()
                        .find(|extension| module_reference.contains(extension))
                        .unwrap_or_default();
                }
                self.report(importing, Diagnostic::with_args(
                    &messages::AN_IMPORT_PATH_CAN_ONLY_END_WITH_A_0_EXTENSION_WHEN_ALLOWIMPORTINGTSEXTENSIONS_IS_ENABLED,
                    span,
                    [ts_extension.to_string()],
                ));
            }
            return false;
        }
        // `sourceFile == nil`. TS7016's arm (`:15376`) is the caller's.
        let Some(message) = diagnostic else { return false };
        if message.code()
            == messages::COULD_NOT_FIND_A_DECLARATION_FILE_FOR_MODULE_0_1_IMPLICITLY_HAS_AN_ANY_TYPE
                .code()
        {
            return false;
        }
        // `moduleNotFoundError != nil` → `c.error(errorNode, resolutionDiagnostic, …)`
        // (`:15403`). Every caller here passes the not-found message.
        self.report(
            importing,
            Diagnostic::with_args(
                message,
                span,
                [module_reference.to_string(), resolved_file_name.clone()],
            ),
        );
        true
    }

    /// `ast.IsEmittableImport` (`ast/utilities.go:3190`).
    fn is_emittable_import(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::ImportDeclaration(import)) => matches!(import.import_clause,
                Some(clause) if clause.phase_modifier.is_none_or(|token| token.kind != SyntaxKind::TypeKeyword)),
            Some(Node::ExportDeclaration(export)) => !export.is_type_only,
            Some(Node::ImportEqualsDeclaration(import)) => !import.is_type_only,
            Some(Node::CallExpression(call)) => {
                matches!(call.expression, Some(tsr_ast::Expression::KeywordExpression(keyword))
                    if keyword.kind == SyntaxKind::ImportKeyword)
            }
            _ => false,
        }
    }

    /// `compilerOptions.AllowImportingTsExtensionsFrom(fileName)`
    /// (`core/compileroptions.go:262`).
    fn allow_importing_ts_extensions_from(
        &self,
        file: NodeId,
        options: ResolutionDiagnosticOptions,
    ) -> bool {
        options.allow_importing_ts_extensions
            || self
                .module_host
                .and_then(|host| host.file_path(file))
                .is_some_and(|path| tsr_path::is_declaration_file_name(&path))
    }

    /// `getSuggestedImportSource` (`checker.go:15438`).
    fn suggested_import_source(
        &self,
        module_reference: &str,
        ts_extension: &str,
        mode: tsr_core::ResolutionMode,
        options: ResolutionDiagnosticOptions,
    ) -> String {
        let without = tsr_path::remove_extension(module_reference, ts_extension);
        let non_node_esm = self.module_kind >= tsr_core::ModuleKind::ES2015
            && self.module_kind <= tsr_core::ModuleKind::ESNext;
        if !(non_node_esm || mode == tsr_core::ModuleKind::ESNext) {
            return without.to_string();
        }
        let prefer_ts = tsr_path::is_declaration_file_name(module_reference)
            && options.allow_importing_ts_extensions;
        let extension = match ts_extension {
            ".mts" | ".d.mts" => {
                if prefer_ts {
                    ".mts"
                } else {
                    ".mjs"
                }
            }
            ".cts" | ".d.cts" => {
                if prefer_ts {
                    ".cts"
                } else {
                    ".cjs"
                }
            }
            _ => {
                if prefer_ts {
                    ".ts"
                } else {
                    ".js"
                }
            }
        };
        format!("{without}{extension}")
    }

    /// `getVerbatimModuleSyntaxErrorMessage` (`checker.go:5681`).
    fn verbatim_module_syntax_error_message(
        &self,
        file: NodeId,
    ) -> &'static tsr_diagnostics::Message {
        let commonjs_extension = self
            .module_host
            .and_then(|host| host.file_path(file))
            .is_some_and(|path| tsr_path::file_extension_is_one_of(&path, &[".cts", ".cjs"]));
        if commonjs_extension {
            &messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBATIMMODULESYNTAX
        } else {
            &messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBATIMMODULESYNTAX_ADJUST_THE_TYPE_FIELD_IN_THE_NEAREST_PACKAGE_JSON_TO_MAKE_THIS_FILE_AN_ECMASCRIPT_MODULE_OR_ADJUST_YOUR_VERBATIMMODULESYNTAX_MODULE_AND_MODULERESOLUTION_SETTINGS_IN_TYPESCRIPT
        }
    }

    /// `getExportSymbolOfValueSymbolIfExported` (`checker.go:14383`).
    fn export_symbol_of_value_symbol_if_exported(&self, symbol: SymbolId) -> SymbolId {
        let entry = self.binder.symbols().get(symbol);
        let symbol = match entry.export_symbol {
            Some(export) if entry.flags.intersects(SymbolFlags::EXPORT_VALUE) => export,
            _ => symbol,
        };
        self.binder.merged_symbol(symbol)
    }

    /// `getSymbolFlagsEx(symbol, excludeTypeOnlyMeanings,
    /// excludeLocalMeanings)` (`checker.go:16367`). The walk is
    /// [`Checker::get_symbol_flags`]'s (which is this with both `false`) plus
    /// upstream's two exclusions and its `getExportSymbolOfValueSymbolIfExported`
    /// of each target. An unresolvable hop ends the walk there, as in that
    /// port; upstream's `unknownSymbol` answers `All`.
    fn symbol_flags_ex(
        &mut self,
        symbol: SymbolId,
        exclude_type_only_meanings: bool,
        exclude_local_meanings: bool,
    ) -> SymbolFlags {
        let mut seen: Vec<SymbolId> = Vec::new();
        let mut current = symbol;
        let mut flags = if exclude_local_meanings {
            SymbolFlags::empty()
        } else {
            self.binder.symbols().get(symbol).flags
        };
        while self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
            if exclude_type_only_meanings
                && self.type_only_alias_declaration_node(current).is_some()
            {
                break;
            }
            let Some(target) = self.resolve_alias(current) else { break };
            let target = self.export_symbol_of_value_symbol_if_exported(target);
            let target_flags = self.binder.symbols().get(target).flags;
            if target_flags.intersects(SymbolFlags::ALIAS) {
                if target == current || seen.contains(&target) {
                    break;
                }
                if seen.is_empty() {
                    seen.push(current);
                }
                seen.push(target);
            }
            flags |= target_flags;
            current = target;
        }
        flags
    }

    /// `getTypeOnlyAliasDeclarationEx(symbol, meaning)` (`checker.go:2143`):
    /// while the hop is an alias **with no `meaning`**, answer its
    /// `aliasSymbolLinks.typeOnlyDeclaration`
    /// ([`Checker::type_only_alias_declaration_node`]), else step to its
    /// target. An alias merged with a local value (`import { A }` beside
    /// `const A`) ends the walk under `Value`, whatever its import says; the
    /// same condition as `check.rs`'s [`Checker::type_only_alias_declaration`]
    /// (`r6-smallcodes4`). Each hop's link already covers the pure-alias hops
    /// behind it, so re-reading them on the way is redundant, never wrong.
    fn type_only_alias_declaration_node_ex(
        &mut self,
        symbol: SymbolId,
        meaning: SymbolFlags,
    ) -> Option<NodeId> {
        let mut current = symbol;
        for _ in 0..16 {
            let flags = self.binder.symbols().get(current).flags;
            if !flags.intersects(SymbolFlags::ALIAS) || flags.intersects(meaning) {
                return None;
            }
            if let Some(declaration) = self.type_only_alias_declaration_node(current) {
                return Some(declaration);
            }
            current = self.resolve_alias(current)?;
        }
        None
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

    /// `getTypeOnlyAliasDeclaration(symbol)` (`checker.go:2133`; this
    /// comment cited `:1861`, the TS1361 caller, until r6-modules2) as the
    /// declaration itself: the first alias declaration on the chain that is
    /// type-only, or that reaches its name through a type-only `export *`.
    /// Bounded like [`Checker::type_only_alias_declaration`], which answers
    /// only *which kind* it found.
    ///
    /// The link is the alias's own type-only declaration
    /// (`markSymbolOfAliasDeclarationIfTypeOnly`, `checker.go:15083`), else the
    /// link `resolveIndirectionAlias` (`:16293`) copies from its target. That
    /// copy happens only when the target is a pure alias,
    /// `ast.IsNonLocalAlias(target, Value|Type|Namespace)` (`:16280`): an alias
    /// merged with a local meaning keeps the chain behind it out of the link.
    fn type_only_alias_declaration_node(&mut self, symbol: SymbolId) -> Option<NodeId> {
        let mut current = self.binder.merged_symbol(symbol);
        if !self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
            return None;
        }
        for _ in 0..16 {
            let declaration = *self.binder.symbols().get(current).declarations.first()?;
            if self.is_type_only_import_or_export_declaration(declaration) {
                return Some(declaration);
            }
            if let Some(star) = self.specifier_type_only_export_star(declaration) {
                return Some(star);
            }
            current = self.binder.merged_symbol(self.resolve_alias(current)?);
            if !self.is_non_local_alias(
                current,
                SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
            ) {
                return None;
            }
        }
        None
    }

    /// `ast.IsNonLocalAlias(symbol, excludes)` (`ast/utilities.go:2608`): an
    /// alias with none of `excludes`, or an assignment alias.
    fn is_non_local_alias(&self, symbol: SymbolId, excludes: SymbolFlags) -> bool {
        let flags = self.binder.symbols().get(symbol).flags;
        flags & (SymbolFlags::ALIAS | excludes) == SymbolFlags::ALIAS
            || flags.contains(SymbolFlags::ALIAS | SymbolFlags::ASSIGNMENT)
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

/// `getParameterTypeNodeForDecoratorCheck` (`checker.go:28734`): a rest
/// parameter's element type (`ast.GetRestParameterElementType`).
fn parameter_type_node_for_decorator_check<'n>(
    parameter: &tsr_ast::ParameterDeclaration<'n>,
) -> Option<TypeNode<'n>> {
    let type_node = parameter.r#type;
    if parameter.dot_dot_dot_token.is_none() {
        return type_node;
    }
    match type_node? {
        TypeNode::ArrayTypeNode(array) => array.element_type,
        TypeNode::TypeReferenceNode(reference) => reference.type_arguments.first().copied(),
        _ => None,
    }
}

/// `getAnnotatedAccessorTypeNode` (`checker.go:20106`) for an accessor: a
/// getter's return annotation, a setter's value parameter's
/// (`getEffectiveSetAccessorTypeAnnotationNode`, skipping a `this`
/// parameter as `GetSetAccessorValueParameter` does).
fn annotated_accessor_type_node(accessor: Node<'_>) -> Option<TypeNode<'_>> {
    match accessor {
        Node::GetAccessorDeclaration(getter) => getter.r#type,
        Node::SetAccessorDeclaration(setter) => {
            let parameters = setter.parameters;
            let has_this = parameters.len() == 2
                && matches!(parameters[0].name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this");
            parameters.get(usize::from(has_this)).and_then(|parameter| parameter.r#type)
        }
        _ => None,
    }
}

/// What `resolveExternalModule` reads off the program's `ResolvedModule`
/// for [`Checker::check_module_resolution_diagnostic`]. `ResolvedFileName`
/// is asked of the host only when a report needs it.
#[allow(
    dead_code,
    reason = "built by the held hook docs/parity/notes/r6-isolated-resolution-diagnostic.diff"
)]
pub(crate) struct ResolvedModuleFacts<'s> {
    /// `Extension`: the resolver's, e.g. `.d.html.ts` for an arbitrary
    /// extension's declaration file, which the path alone cannot tell from
    /// a `.ts` file.
    pub(crate) extension: &'s str,
    /// `ResolvedUsingTsExtension`.
    pub(crate) resolved_using_ts_extension: bool,
    /// `GetSourceFileForResolvedModule(ResolvedFileName) != nil`.
    pub(crate) in_program: bool,
}

/// The options `GetResolutionDiagnostic` and `AllowImportingTsExtensionsFrom`
/// read beyond the ones `Checker` already keeps (`jsx`, `noImplicitAny`).
#[allow(
    dead_code,
    reason = "built by the held hook docs/parity/notes/r6-isolated-resolution-diagnostic.diff"
)]
#[derive(Clone, Copy, Debug, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent compiler options, as upstream reads them"
)]
pub(crate) struct ResolutionDiagnosticOptions {
    /// `GetAllowJS()`.
    pub(crate) allow_js: bool,
    /// `GetResolveJsonModule()`.
    pub(crate) resolve_json_module: bool,
    /// `AllowArbitraryExtensions.IsTrue()`.
    pub(crate) allow_arbitrary_extensions: bool,
    /// `GetAllowImportingTsExtensions()`: the option or
    /// `rewriteRelativeImportExtensions`.
    pub(crate) allow_importing_ts_extensions: bool,
}

/// `module.GetResolutionDiagnostic` (`module/util.go:123`).
/// `importing_is_declaration` is `file.IsDeclarationFile`, asked only by the
/// arbitrary-extension arm; `jsx_none` is `options.Jsx == JsxEmitNone`;
/// `no_implicit_any` is `NoImplicitAny.DefaultIfUnknown(Strict)`.
#[allow(
    dead_code,
    reason = "called through the held hook docs/parity/notes/r6-isolated-resolution-diagnostic.diff"
)]
fn resolution_diagnostic(
    extension: &str,
    importing_is_declaration: impl FnOnce() -> bool,
    jsx_none: bool,
    no_implicit_any: bool,
    options: ResolutionDiagnosticOptions,
) -> Option<&'static tsr_diagnostics::Message> {
    let need_jsx = || jsx_none.then_some(&messages::MODULE_0_WAS_RESOLVED_TO_1_BUT_JSX_IS_NOT_SET);
    let need_allow_js = || {
        (!options.allow_js && no_implicit_any).then_some(
            &messages::COULD_NOT_FIND_A_DECLARATION_FILE_FOR_MODULE_0_1_IMPLICITLY_HAS_AN_ANY_TYPE,
        )
    };
    match extension {
        ".ts" | ".d.ts" | ".mts" | ".d.mts" | ".cts" | ".d.cts" => None,
        ".tsx" => need_jsx(),
        ".jsx" => need_jsx().or_else(need_allow_js),
        ".js" | ".mjs" | ".cjs" => need_allow_js(),
        ".json" => (!options.resolve_json_module)
            .then_some(&messages::MODULE_0_WAS_RESOLVED_TO_1_BUT_RESOLVEJSONMODULE_IS_NOT_USED),
        _ => (!options.allow_arbitrary_extensions && !importing_is_declaration()).then_some(
            &messages::MODULE_0_WAS_RESOLVED_TO_1_BUT_ALLOWARBITRARYEXTENSIONS_IS_NOT_SET,
        ),
    }
}
