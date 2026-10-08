//! Module-format checks: the diagnostics upstream gates on a file's **emit
//! module format** (`Program.GetEmitModuleFormatOfFile`,
//! `Program.GetImpliedNodeFormatForEmit`) or on `c.moduleKind`, rather than on
//! the syntax alone.
//!
//! Each check names its pinned counterpart (`vendor/typescript-go` @
//! `5b1047d`). They are reached from one hook in the check walk
//! ([`Checker::check_module_format`]) plus the export-assignment arm in
//! `check.rs`. No cache, side table or traversal of their own: every check
//! is a position test on the node the walk hands it plus, where upstream asks
//! the program, one [`ModuleHost`](crate::resolution::ModuleHost) query.
//! `docs/parity/notes/r5-modfmt.md`.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_core::{ModuleKind, ResolutionMode};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::check::has_modifier;

/// The three spellings `checkGrammarAwaitOrAwaitUsing` (`grammarchecks.go:1676`)
/// and `checkGrammarForInOrForOfStatement` (`:1205`) report on at top level;
/// each has its own message per arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopLevelAwait {
    Expression,
    AwaitUsing,
    ForAwait,
}

impl TopLevelAwait {
    /// The `!IsEffectiveExternalModule` message (TS1375 / TS2853 / TS1431).
    fn not_a_module(self) -> &'static Message {
        match self {
            Self::Expression => &messages::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A_MODULE_BUT_THIS_FILE_HAS_NO_IMPORTS_OR_EXPORTS_CONSIDER_ADDING_AN_EMPTY_EXPORT_TO_MAKE_THIS_FILE_A_MODULE,
            Self::AwaitUsing => &messages::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A_MODULE_BUT_THIS_FILE_HAS_NO_IMPORTS_OR_EXPORTS_CONSIDER_ADDING_AN_EMPTY_EXPORT_TO_MAKE_THIS_FILE_A_MODULE,
            Self::ForAwait => &messages::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS_A_MODULE_BUT_THIS_FILE_HAS_NO_IMPORTS_OR_EXPORTS_CONSIDER_ADDING_AN_EMPTY_EXPORT_TO_MAKE_THIS_FILE_A_MODULE,
        }
    }

    /// The module/target message (TS1378 / TS2854 / TS1432).
    fn unsupported(self) -> &'static Message {
        match self {
            Self::Expression => &messages::TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES2022_ESNEXT_SYSTEM_NODE16_NODE18_NODE20_NODENEXT_OR_PRESERVE_AND_THE_TARGET_OPTION_IS_SET_TO_ES2017_OR_HIGHER,
            Self::AwaitUsing => &messages::TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES2022_ESNEXT_SYSTEM_NODE16_NODE18_NODE20_NODENEXT_OR_PRESERVE_AND_THE_TARGET_OPTION_IS_SET_TO_ES2017_OR_HIGHER,
            Self::ForAwait => &messages::TOP_LEVEL_FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES2022_ESNEXT_SYSTEM_NODE16_NODE18_NODE20_NODENEXT_OR_PRESERVE_AND_THE_TARGET_OPTION_IS_SET_TO_ES2017_OR_HIGHER,
        }
    }
}
use crate::checker::Checker;

/// The compiler options the module-format checks read, captured once by
/// `apply_compiler_options` (ADR-0042's pattern: the checker copies what it
/// reads rather than holding the options).
// One bool per compiler option read, as `CompilerOptions` itself holds them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleFormatOptions {
    /// `GetEmitModuleKind()`, upstream's `c.moduleKind` (`checker.go:880`).
    /// Not the checker's shared `module_kind`, which maps every unset module
    /// at `target >= es2015` to `ES2015` where upstream climbs to
    /// `ES2020`/`ES2022`/`ESNext` (`r5-modfmt.md` §5).
    pub(crate) module_kind: ModuleKind,
    /// `GetEmitModuleDetectionKind()`.
    pub(crate) module_detection: tsr_core::ModuleDetectionKind,
    /// `compilerOptions.AllowUmdGlobalAccess.IsTrue()`: TS2686 becomes a
    /// suggestion (`checker.go:1846`).
    pub(crate) allow_umd_global_access: bool,
    /// `compilerOptions.ErasableSyntaxOnly.IsTrue()` (`shouldCheckErasableSyntax`,
    /// `checker.go:2650`, with its not-a-JS-file half).
    pub(crate) erasable_syntax_only: bool,
    /// `compilerOptions.NoFallthroughCasesInSwitch.IsTrue()` (`checker.go:4196`).
    pub(crate) no_fallthrough_cases_in_switch: bool,
    /// `compilerOptions.NoEmit.IsTrue()`: `errorSkippedOnNoEmit` and
    /// `grammarErrorOnNodeSkippedOnNoEmit` diagnostics are dropped
    /// (`SetSkippedOnNoEmit`, filtered by the program under `noEmit`).
    pub(crate) no_emit: bool,
}

impl Default for ModuleFormatOptions {
    /// A checker that never sees `apply_compiler_options` keeps the shared
    /// `module_kind`'s `None`, and [`Checker::check_module_format`] is inert
    /// for it: no program, no format to check.
    fn default() -> Self {
        Self {
            module_kind: ModuleKind::None,
            module_detection: tsr_core::ModuleDetectionKind::Auto,
            allow_umd_global_access: false,
            erasable_syntax_only: false,
            no_fallthrough_cases_in_switch: false,
            no_emit: false,
        }
    }
}

impl ModuleFormatOptions {
    pub(crate) fn from_options(options: &tsr_core::CompilerOptions) -> Self {
        Self {
            module_kind: options.emit_module_kind(),
            module_detection: options.emit_module_detection_kind(),
            allow_umd_global_access: options.allow_umd_global_access.is_true(),
            erasable_syntax_only: options.erasable_syntax_only.is_true(),
            no_fallthrough_cases_in_switch: options.no_fallthrough_cases_in_switch.is_true(),
            no_emit: options.no_emit.is_true(),
        }
    }
}

impl Checker<'_, '_> {
    /// The check walk's hook: every node, after its own `ambient` context is
    /// known. Dispatches to the native check that owns each module-format
    /// diagnostic for that node kind.
    pub(crate) fn check_module_format(&mut self, node: NodeId, typed: Node<'_>, ambient: bool) {
        if self.module_format_options.module_kind == ModuleKind::None {
            return;
        }
        if self.module_format_options.erasable_syntax_only {
            self.check_erasable_syntax(node, typed, ambient);
        }
        match typed {
            Node::ImportEqualsDeclaration(declaration) => {
                self.check_import_equals_module_format(node, declaration, ambient);
                self.check_collisions_in_generated_code(node, ambient);
            }
            Node::FunctionDeclaration(_)
            | Node::EnumDeclaration(_)
            | Node::ModuleDeclaration(_)
            | Node::ImportClause(_)
            | Node::NamespaceImport(_)
            | Node::BindingElement(_) => self.check_collisions_in_generated_code(node, ambient),
            Node::ImportSpecifier(specifier) => {
                self.check_collisions_in_generated_code(node, ambient);
                if let Some(name) = specifier.property_name.and_then(|n| n.node_id()) {
                    self.check_module_export_name(node, name);
                }
            }
            Node::ExportSpecifier(specifier) => {
                // `checkExportSpecifier` (`checker.go:5551`): the property name
                // may be a string only with a module specifier (otherwise it is
                // TS1003, not this check's); the exported name always may.
                let has_module_specifier =
                    self.nodes.parent(node).and_then(|named| self.nodes.parent(named)).is_some_and(
                        |declaration| {
                            matches!(self.node_map.get(declaration),
                            Some(Node::ExportDeclaration(d)) if d.module_specifier.is_some())
                        },
                    );
                if has_module_specifier
                    && let Some(name) = specifier.property_name.and_then(|n| n.node_id())
                {
                    self.check_module_export_name(node, name);
                }
                if let Some(name) = specifier.name.and_then(|n| n.node_id()) {
                    self.check_module_export_name(node, name);
                }
            }
            Node::NamespaceExport(export) => {
                if let Some(name) = export.name.and_then(|n| n.node_id()) {
                    self.check_module_export_name(node, name);
                }
            }
            Node::VariableDeclaration(declaration) => {
                self.check_collisions_in_generated_code(node, ambient);
                self.check_es_module_marker(node, declaration, ambient);
            }
            Node::ClassDeclaration(_) | Node::ClassExpression(_) => {
                self.check_collisions_in_generated_code(node, ambient);
                self.check_class_name_collision_with_object(node, ambient);
            }
            Node::ArrowFunction(arrow) => self.check_reserved_arrow_type_parameters(arrow),
            Node::TypeAssertion(_) => self.check_reserved_type_assertion(node),
            Node::CaseOrDefaultClause(clause) => self.check_fallthrough_case(node, clause),
            Node::AwaitExpression(_) => {
                let start = self.nodes.span(node).start;
                self.check_top_level_await(node, TopLevelAwait::Expression, start);
            }
            Node::ForInOrOfStatement(statement) => {
                if self.nodes.kind(node) == SyntaxKind::ForOfStatement
                    && let Some(modifier) = statement.await_modifier.and_then(|m| m.node_id)
                {
                    let start = self.nodes.span(modifier).start;
                    self.check_top_level_await(node, TopLevelAwait::ForAwait, start);
                }
            }
            Node::VariableStatement(statement) => {
                if let Some(start) = self.await_using_keyword_start(node, statement) {
                    self.check_top_level_await(node, TopLevelAwait::AwaitUsing, start);
                }
            }
            _ => {}
        }
    }

    /// `Program.GetImpliedNodeFormatForEmit` (`program.go:1554`); `None`
    /// without a host, which is what a program with no per-file formats
    /// answers for every file.
    pub(crate) fn implied_node_format_for_emit_of(&self, file: NodeId) -> ResolutionMode {
        self.module_host.map_or(ModuleKind::None, |host| host.implied_node_format_for_emit(file))
    }

    /// `Program.GetEmitModuleFormatOfFile` (`ast.GetEmitModuleFormatOfFileWorker`):
    /// the implied format for emit, else the emit module kind.
    pub(crate) fn emit_module_format_of(&self, file: NodeId) -> ModuleKind {
        match self.implied_node_format_for_emit_of(file) {
            ModuleKind::None => self.module_format_options.module_kind,
            format => format,
        }
    }

    fn error_on_node(&mut self, node: NodeId, message: &'static Message, args: Vec<String>) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, args));
    }

    /// TS1202 — `checkImportEqualsDeclaration`'s external-reference arm
    /// (`checker.go:5494`):
    ///
    /// ```go
    /// if core.ModuleKindES2015 <= c.moduleKind && c.moduleKind <= core.ModuleKindESNext && !node.IsTypeOnly() && node.Flags&ast.NodeFlagsAmbient == 0 {
    /// ```
    ///
    /// Gated on `c.moduleKind`, **not** the file's format: a `.cts` under
    /// `module: esnext` still reports (`impliedNodeFormatEmit1`). Reached past
    /// `checkGrammarModuleElementContext` (`:5465`) and
    /// `checkExternalImportOrExportDeclaration` (`:5333`), whose passing
    /// conditions are the position tests below.
    fn check_import_equals_module_format(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::ImportEqualsDeclaration<'_>,
        ambient: bool,
    ) {
        if !(ModuleKind::ES2015..=ModuleKind::ESNext)
            .contains(&self.module_format_options.module_kind)
            || declaration.is_type_only
            || ambient
        {
            return;
        }
        let Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) =
            declaration.module_reference
        else {
            return;
        };
        // `checkExternalImportOrExportDeclaration`: a string-literal name,
        // in a source file or an ambient module's block.
        let Some(name) = reference.expression.and_then(|e| e.node_id()) else { return };
        let span = self.nodes.span(name);
        if self.nodes.kind(name) != SyntaxKind::StringLiteral || span.start == span.end {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        match self.nodes.kind(parent) {
            SyntaxKind::SourceFile => {}
            SyntaxKind::ModuleBlock => {
                let Some(module) = self.nodes.parent(parent) else { return };
                // `ast.IsAmbientModule`: a string-literal name. A relative
                // specifier inside one is rejected there (TS2439) unless the
                // block is a top-level augmentation; declined either way, as
                // an ambient module's contents are ambient.
                let is_ambient_module = matches!(self.node_map.get(module),
                    Some(Node::ModuleDeclaration(m)) if matches!(m.name, Some(tsr_ast::ModuleName::StringLiteral(_))));
                if !is_ambient_module {
                    return;
                }
            }
            _ => return,
        }
        // `grammarErrorOnNode`: silent in a file with parse diagnostics.
        if self.file_has_parse_errors {
            return;
        }
        self.grammar_error_on_node(
            node,
            &messages::IMPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USING_IMPORT_ASTERISK_AS_NS_FROM_MOD_IMPORT_A_FROM_MOD_IMPORT_D_FROM_MOD_OR_ANOTHER_MODULE_FORMAT_INSTEAD,
        );
    }

    /// TS1203 / TS1218 — `checkExportAssignment`'s `export =` tail
    /// (`checker.go:5669-5678`), for an `export =` whose context checks
    /// passed. Answers whether a diagnostic was reported.
    ///
    /// ```go
    /// if c.moduleKind >= core.ModuleKindES2015 && c.moduleKind != core.ModuleKindPreserve &&
    ///     ((ambient && GetImpliedNodeFormatForEmit(file) == ESNext) ||
    ///      (!ambient && GetImpliedNodeFormatForEmit(file) != CommonJS)) { TS1203 }
    /// else if c.moduleKind == core.ModuleKindSystem && !ambient { TS1218 }
    /// ```
    pub(crate) fn check_export_equals_module_format(
        &mut self,
        node: NodeId,
        file: NodeId,
        ambient: bool,
    ) -> bool {
        let format = self.implied_node_format_for_emit_of(file);
        let message = if self.module_format_options.module_kind >= ModuleKind::ES2015
            && self.module_format_options.module_kind != ModuleKind::Preserve
            && ((ambient && format == ModuleKind::ESNext)
                || (!ambient && format != ModuleKind::CommonJS))
        {
            &messages::EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USING_EXPORT_DEFAULT_OR_ANOTHER_MODULE_FORMAT_INSTEAD
        } else if self.module_format_options.module_kind == ModuleKind::System && !ambient {
            &messages::EXPORT_ASSIGNMENT_IS_NOT_SUPPORTED_WHEN_MODULE_FLAG_IS_SYSTEM
        } else {
            return false;
        };
        if self.file_has_parse_errors {
            return true;
        }
        let span = self.nodes.span(node);
        self.report(file, Diagnostic::new(message, span));
        true
    }

    /// `checkCollisionsForDeclarationName` (`checker.go:10444`)'s two
    /// format-gated arms: `checkCollisionWithRequireExportsInGeneratedCode`
    /// (`:10463`) and `checkCollisionWithGlobalObjectInGeneratedCode`
    /// (`:10482`), both TS2441. Reached for the declaration kinds whose
    /// `checkXxx` calls it (`checkFunctionDeclaration`, `checkClassLikeDeclaration`,
    /// `checkEnumDeclaration`, `checkModuleDeclaration` for an identifier
    /// name, `checkImportBinding`, `checkVariableLikeDeclaration`); both arms
    /// need the declaration container to be the source file, which no
    /// function expression or parameter has.
    fn check_collisions_in_generated_code(&mut self, node: NodeId, ambient: bool) {
        let Some(name) = self.collision_declaration_name(node) else { return };
        let Some(Node::Identifier(identifier)) = self.node_map.get(name) else { return };
        let text = identifier.text;
        let class_like = matches!(
            self.nodes.kind(node),
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
        );
        let require_or_exports = text == "require" || text == "exports";
        let object = text == "Object" && !class_like;
        if !require_or_exports && !object {
            return;
        }
        if !self.need_collision_check_for_identifier_in_module(node, ambient) {
            return;
        }
        // Uninstantiated modules do not emit a binding.
        if self.nodes.kind(node) == SyntaxKind::ModuleDeclaration
            && !self.module_declaration_is_instantiated(node)
        {
            return;
        }
        let Some(container) = self.generated_code_declaration_container(node) else { return };
        if self.nodes.kind(container) != SyntaxKind::SourceFile
            || !self.is_external_or_common_js_module(container)
        {
            return;
        }
        let format = self.emit_module_format_of(container);
        let reports = if require_or_exports {
            // "No need to check for require or exports for ES6 modules and later".
            format < ModuleKind::ES2015
        } else {
            format == ModuleKind::CommonJS
        };
        if !reports || self.module_format_options.no_emit {
            return;
        }
        self.error_on_node(
            name,
            &messages::DUPLICATE_IDENTIFIER_0_COMPILER_RESERVES_NAME_1_IN_TOP_LEVEL_SCOPE_OF_A_MODULE,
            vec![text.to_string(), text.to_string()],
        );
    }

    /// TS2725 — `checkClassNameCollisionWithObject` (`checker.go:10611`),
    /// called from `checkCollisionsForDeclarationName` for a non-ambient
    /// class-like declaration.
    fn check_class_name_collision_with_object(&mut self, node: NodeId, ambient: bool) {
        if ambient {
            return;
        }
        let Some(name) = self.collision_declaration_name(node) else { return };
        let Some(Node::Identifier(identifier)) = self.node_map.get(name) else { return };
        if identifier.text != "Object" {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if self.emit_module_format_of(file) >= ModuleKind::ES2015 {
            return;
        }
        self.error_on_node(
            name,
            &messages::CLASS_NAME_CANNOT_BE_OBJECT_WHEN_TARGETING_ES5_AND_ABOVE_WITH_MODULE_0,
            vec![format!("{:?}", self.module_format_options.module_kind)],
        );
    }

    /// TS1216 — `checkGrammarVariableDeclaration`'s marker arm
    /// (`grammarchecks.go:1600`) and `checkGrammarForEsModuleMarkerInBindingName`
    /// (`:1614`): an exported, non-ambient variable statement in a file
    /// emitted below `System` may not bind `__esModule`.
    fn check_es_module_marker(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::VariableDeclaration<'_>,
        ambient: bool,
    ) {
        if ambient || self.file_has_parse_errors || self.module_format_options.no_emit {
            return;
        }
        // `node.Parent.Parent`: the statement holding the declaration list.
        let Some(statement) = self.nodes.parent(node).and_then(|list| self.nodes.parent(list))
        else {
            return;
        };
        let Some(Node::VariableStatement(statement_node)) = self.node_map.get(statement) else {
            return;
        };
        if !has_modifier(statement_node.modifiers, SyntaxKind::ExportKeyword) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if self.emit_module_format_of(file) >= ModuleKind::System {
            return;
        }
        let Some(name) = declaration.name.and_then(|n| n.node_id()) else { return };
        if let Some(marker) = self.es_module_marker_in_binding_name(name) {
            self.error_on_node(
                marker,
                &messages::IDENTIFIER_EXPECTED_ESMODULE_IS_RESERVED_AS_AN_EXPORTED_MARKER_WHEN_TRANSFORMING_ECMASCRIPT_MODULES,
                Vec::new(),
            );
        }
    }

    /// `checkGrammarForEsModuleMarkerInBindingName`'s search: an identifier
    /// name is tested; a pattern recurses into its **first** element that has
    /// a name, and stops there (`return` inside the loop).
    fn es_module_marker_in_binding_name(&self, name: NodeId) -> Option<NodeId> {
        match self.node_map.get(name)? {
            Node::Identifier(identifier) => (identifier.text == "__esModule").then_some(name),
            Node::BindingPattern(_) => {
                let mut elements = Vec::new();
                tsr_ast::for_each_child_id(self.node_map.get(name)?, |child| elements.push(child));
                let first_named = elements.into_iter().find_map(|element| {
                    match self.node_map.get(element)? {
                        Node::BindingElement(binding) => binding.name.and_then(|n| n.node_id()),
                        _ => None,
                    }
                })?;
                self.es_module_marker_in_binding_name(first_named)
            }
            _ => None,
        }
    }

    /// The `name` argument each caller of `checkCollisionsForDeclarationName`
    /// passes (`node.Name()`), for the kinds [`Checker::check_module_format`]
    /// routes here. A module declaration only with an identifier name
    /// (`checker.go:5155`).
    fn collision_declaration_name(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::ImportEqualsDeclaration(n) => n.name.and_then(|name| name.node_id),
            Node::ImportClause(n) => n.name.and_then(|name| name.node_id),
            Node::NamespaceImport(n) => n.name.and_then(|name| name.node_id),
            Node::ImportSpecifier(n) => n.name.and_then(|name| name.node_id),
            _ => self.declaration_name_of(node),
        }
    }

    /// `needCollisionCheckForIdentifier` (`checker.go:10500`) after its name
    /// test, as the module-collision checks call it. `class_fields.rs` holds a
    /// second port of the same function for the class-field checks; the two
    /// are to be unified (`tsr-2zk.1022`).
    ///
    /// test: no member kinds reach here; an ambient declaration and a
    /// type-only import have no emit; an overload parameter has none either
    /// but has no source-file container, so it never reaches the reports.
    fn need_collision_check_for_identifier_in_module(&self, node: NodeId, ambient: bool) -> bool {
        if ambient {
            return false;
        }
        match self.node_map.get(node) {
            Some(Node::ImportEqualsDeclaration(n)) => !n.is_type_only,
            Some(Node::ImportClause(clause)) => !Self::import_clause_is_type_only(clause),
            Some(Node::ImportSpecifier(specifier)) => {
                !specifier.is_type_only
                    && !self
                        .nodes
                        .parent(node)
                        .and_then(|named| self.nodes.parent(named))
                        .and_then(|clause| match self.node_map.get(clause) {
                            Some(Node::ImportClause(clause)) => {
                                Some(Self::import_clause_is_type_only(clause))
                            }
                            _ => None,
                        })
                        .unwrap_or(false)
            }
            _ => true,
        }
    }

    fn import_clause_is_type_only(clause: &tsr_ast::ImportClause<'_>) -> bool {
        clause.phase_modifier.is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
    }

    /// `ast.GetDeclarationContainer` (`ast/utilities.go:2590`): from the root
    /// declaration (past binding elements), the first ancestor that is not a
    /// variable declaration, declaration list or import piece, then its
    /// parent.
    fn generated_code_declaration_container(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        while self.nodes.kind(current) == SyntaxKind::BindingElement {
            current = self.nodes.parent(current).and_then(|pattern| self.nodes.parent(pattern))?;
        }
        while matches!(
            self.nodes.kind(current),
            SyntaxKind::VariableDeclaration
                | SyntaxKind::VariableDeclarationList
                | SyntaxKind::ImportSpecifier
                | SyntaxKind::NamedImports
                | SyntaxKind::NamespaceImport
                | SyntaxKind::ImportClause
        ) {
            current = self.nodes.parent(current)?;
        }
        self.nodes.parent(current)
    }

    /// `ast.IsExternalOrCommonJSModule`: the binder creates a file symbol
    /// exactly for an external module or a `CommonJS` module
    /// (`bindSourceFileAsExternalModule`; ADR-0041).
    pub(crate) fn is_external_or_common_js_module(&self, file: NodeId) -> bool {
        self.binder.symbol_of(file).is_some()
    }

    /// `ast.GetModuleInstanceState(node) == ModuleInstanceStateInstantiated`.
    fn module_declaration_is_instantiated(&self, node: NodeId) -> bool {
        let Some(typed) = self.node_map.get(node) else { return true };
        let mut parents: Vec<_> =
            self.nodes.ancestors(node).filter_map(|ancestor| self.node_map.get(ancestor)).collect();
        parents.reverse();
        tsr_ast::module_instance_state(typed, &parents)
            == tsr_ast::ModuleInstanceState::Instantiated
    }

    /// The top-level arm of `checkGrammarAwaitOrAwaitUsing`
    /// (`grammarchecks.go:1689-1741`) and its `for await` twin in
    /// `checkGrammarForInOrForOfStatement` (`:1205-1232`): reached when the
    /// node has no `AwaitContext` and `IsInTopLevelContext`. `start` is where
    /// `GetRangeOfTokenAtPosition(sourceFile, node.Pos())` (or the
    /// `AwaitModifier`) puts the `await` keyword.
    ///
    /// `NodeFlagsAwaitContext` is set by upstream's parser in async bodies and
    /// on top-level statements it **reparses** (`reparseTopLevelAwait`), which
    /// is only ever the ambiguous `await (x)` / `await [x]` spellings that this
    /// port's parser leaves as identifiers. An `AwaitExpression` this port
    /// built at top level therefore never carries it upstream either.
    fn check_top_level_await(&mut self, node: NodeId, kind: TopLevelAwait, start: u32) {
        if self.file_has_parse_errors || !self.is_in_top_level_context(node) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let Some(is_module) = self.is_effective_external_module_for_format(file) else { return };
        // `GetRangeOfTokenAtPosition`: the five-byte `await` keyword.
        let span = tsr_core::Span::new(start, start + 5);
        if !is_module {
            self.report(file, Diagnostic::new(kind.not_a_module(), span));
        }
        let unsupported = match self.module_format_options.module_kind {
            ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 | ModuleKind::NodeNext
                if self.implied_node_format_for_emit_of(file) == ModuleKind::CommonJS =>
            {
                // Under `node16`..`nodenext` the format for emit *is* the
                // metadata's `ImpliedNodeFormat` upstream reads here.
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_AND_CANNOT_USE_AWAIT_AT_THE_TOP_LEVEL,
                        span,
                    ),
                );
                return;
            }
            ModuleKind::Node16
            | ModuleKind::Node18
            | ModuleKind::Node20
            | ModuleKind::NodeNext
            | ModuleKind::ES2022
            | ModuleKind::ESNext
            | ModuleKind::Preserve
            | ModuleKind::System => self.language_version < tsr_core::ScriptTarget::ES2017,
            _ => true,
        };
        if unsupported {
            self.report(file, Diagnostic::new(kind.unsupported(), span));
        }
    }

    /// `ast.IsInTopLevelContext` (`ast/utilities.go:1778`): the node's
    /// `GetThisContainer(node, includeArrowFunctions=true, false)` is the
    /// source file. Computed property names and decorators of class members
    /// skip to the class, as upstream's walk does.
    fn is_in_top_level_context(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            current = parent;
            match self.nodes.kind(current) {
                SyntaxKind::ComputedPropertyName | SyntaxKind::Decorator => {
                    // Both resolve their container from the class element's
                    // parent, which is never itself a `this` container.
                    if let Some(element) = self.nodes.parent(current) {
                        current = element;
                    }
                }
                SyntaxKind::ArrowFunction
                | SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ModuleDeclaration
                | SyntaxKind::ClassStaticBlockDeclaration
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::CallSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::EnumDeclaration => return false,
                SyntaxKind::SourceFile => return true,
                _ => {}
            }
        }
        false
    }

    /// `ast.IsEffectiveExternalModule(file, options)` (`ast/utilities.go:1669`)
    /// with the file's `ExternalModuleIndicator` as the parser sets it under
    /// `moduleDetection` (`GetExternalModuleIndicatorOptions`,
    /// `ast/parseoptions.go:19`): an import/export statement, or — for a
    /// non-declaration file — `Force` (every file under `force`, and under
    /// `auto` a file `isFileForcedToBeModuleByFormat`), or the `JSX` arm.
    ///
    /// `None` where the answer needs a fact this port does not record: the
    /// `auto` + `react-jsx` arm asks whether the file contains a JSX tag.
    fn is_effective_external_module_for_format(&self, file: NodeId) -> Option<bool> {
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return Some(false) };
        if tsr_binder::is_external_module(source) {
            return Some(true);
        }
        // `CommonJSModuleIndicator` under a CommonJS-containing module kind:
        // the binder symbol of a file that is not an ES module.
        if (self.module_format_options.module_kind == ModuleKind::CommonJS
            || (ModuleKind::Node16..=ModuleKind::NodeNext)
                .contains(&self.module_format_options.module_kind))
            && self.is_external_or_common_js_module(file)
        {
            return Some(true);
        }
        let file_name = self.module_host.and_then(|host| host.file_path(file));
        let declaration = file_name.as_deref().is_some_and(tsr_path::is_declaration_file_name);
        if declaration {
            return Some(false);
        }
        match self.module_format_options.module_detection {
            tsr_core::ModuleDetectionKind::Force => Some(true),
            tsr_core::ModuleDetectionKind::Auto => {
                let forced_by_format = self.implied_node_format_for_emit_of(file)
                    == ModuleKind::ESNext
                    || file_name.as_deref().is_some_and(|name| {
                        tsr_path::extension::file_extension_is_one_of(
                            name,
                            &[
                                tsr_path::extension::EXTENSION_CJS,
                                tsr_path::extension::EXTENSION_CTS,
                                tsr_path::extension::EXTENSION_MJS,
                                tsr_path::extension::EXTENSION_MTS,
                            ],
                        )
                    });
                if forced_by_format {
                    Some(true)
                } else if matches!(
                    self.jsx_emit,
                    tsr_core::JsxEmit::ReactJsx | tsr_core::JsxEmit::ReactJsxDev
                ) {
                    None
                } else {
                    Some(false)
                }
            }
            _ => Some(false),
        }
    }

    /// The `await` of an `await using` statement. This port's parser flags
    /// the list `USING` and drops the `await` (`check_grammar_variable_declaration_list`
    /// in `grammar.rs`), so — as there — the `await` is the gap between a
    /// modifier-less statement's start and its list's start; a
    /// `CONST | USING` list (upstream's `NodeFlagsAwaitUsing`) is accepted
    /// for when the parser keeps it.
    fn await_using_keyword_start(
        &self,
        node: NodeId,
        statement: &tsr_ast::VariableStatement<'_>,
    ) -> Option<u32> {
        let list = statement.declaration_list.and_then(|list| list.node_id)?;
        let block_scope = self.nodes.flags(list) & tsr_ast::NodeFlags::BLOCK_SCOPED;
        let statement_start = self.nodes.span(node).start;
        let list_start = self.nodes.span(list).start;
        let awaited = block_scope == tsr_ast::NodeFlags::CONSTANT
            || (block_scope == tsr_ast::NodeFlags::USING
                && statement.modifiers.is_empty()
                && statement_start != list_start);
        awaited.then_some(statement_start.min(list_start))
    }

    /// Whether the file's name ends in `.mts` or `.cts`
    /// (`tspath.FileExtensionIsOneOf(file.FileName(), {Mts, Cts})`).
    fn is_mts_or_cts_file(&self, node: NodeId) -> bool {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return false };
        self.module_host.and_then(|host| host.file_path(file)).is_some_and(|name| {
            tsr_path::extension::file_extension_is_one_of(
                &name,
                &[tsr_path::extension::EXTENSION_MTS, tsr_path::extension::EXTENSION_CTS],
            )
        })
    }

    /// TS7059 — `checkAssertion`'s first arm (`checker.go:12288`): an
    /// angle-bracket assertion in a `.mts`/`.cts` file.
    fn check_reserved_type_assertion(&mut self, node: NodeId) {
        if self.file_has_parse_errors || !self.is_mts_or_cts_file(node) {
            return;
        }
        self.grammar_error_on_node(
            node,
            &messages::THIS_SYNTAX_IS_RESERVED_IN_FILES_WITH_THE_MTS_OR_CTS_EXTENSION_USE_AN_AS_EXPRESSION_INSTEAD,
        );
    }

    /// TS7060 — `checkGrammarArrowFunction` (`grammarchecks.go:772`): a
    /// generic arrow whose only type parameter has no constraint and no
    /// trailing comma reads as a JSX-like `<T>` in a `.mts`/`.cts` file.
    ///
    /// The parser does not record a type-parameter list's trailing comma, so
    /// the comma is read from the source after the parameter — on this error
    /// path only, and declined without a host that can supply the text.
    fn check_reserved_arrow_type_parameters(&mut self, arrow: &tsr_ast::ArrowFunction<'_>) {
        let [only] = arrow.type_parameters else { return };
        if only.constraint.is_some() || self.file_has_parse_errors {
            return;
        }
        let Some(parameter) = only.node_id else { return };
        if !self.is_mts_or_cts_file(parameter) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(parameter) else { return };
        let Some(text) = self.module_host.and_then(|host| host.source_text(file, self.nodes))
        else {
            return;
        };
        let after = self.nodes.span(parameter).end as usize;
        let Some(rest) = text.get(after..) else { return };
        if rest.trim_start().starts_with(',') {
            return;
        }
        self.grammar_error_on_node(
            parameter,
            &messages::THIS_SYNTAX_IS_RESERVED_IN_FILES_WITH_THE_MTS_OR_CTS_EXTENSION_ADD_A_TRAILING_COMMA_OR_EXPLICIT_CONSTRAINT,
        );
    }

    /// TS18057 — `checkModuleExportName(name, allowStringLiteral=true)`
    /// (`checker.go:5388`): a string-literal import/export name under
    /// `module: es2015`/`es2020`, outside a declaration file. Reached from
    /// `checkImportBinding` (import specifiers), `checkExportSpecifier` and
    /// `checkExportDeclaration`'s `export * as "x"` arm, each past its
    /// declaration's context checks — the position test here.
    fn check_module_export_name(&mut self, binding: NodeId, name: NodeId) {
        if self.nodes.kind(name) != SyntaxKind::StringLiteral
            || !matches!(
                self.module_format_options.module_kind,
                ModuleKind::ES2015 | ModuleKind::ES2020
            )
            || self.file_has_parse_errors
        {
            return;
        }
        let Some(declaration) = self.nodes.ancestors(binding).find(|&ancestor| {
            matches!(
                self.nodes.kind(ancestor),
                SyntaxKind::ImportDeclaration | SyntaxKind::ExportDeclaration
            )
        }) else {
            return;
        };
        if !self.module_declaration_context_passes(declaration) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        if self.module_host.is_some_and(|host| host.is_declaration_file(file)) {
            return;
        }
        self.grammar_error_on_node(
            name,
            &messages::STRING_LITERAL_IMPORT_AND_EXPORT_NAMES_ARE_NOT_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2015_OR_ES2020,
        );
    }

    /// `checkGrammarModuleElementContext` (`grammarchecks.go:206`) and, for a
    /// declaration with a module specifier, `checkExternalImportOrExportDeclaration`
    /// (`checker.go:5333`): a string-literal specifier, in a source file or an
    /// ambient module's block.
    fn module_declaration_context_passes(&self, declaration: NodeId) -> bool {
        let specifier = match self.node_map.get(declaration) {
            Some(Node::ImportDeclaration(import)) => import.module_specifier,
            Some(Node::ExportDeclaration(export)) => export.module_specifier,
            _ => return false,
        };
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        let Some(specifier) = specifier.and_then(|s| s.node_id()) else {
            return matches!(
                self.nodes.kind(parent),
                SyntaxKind::SourceFile | SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration
            );
        };
        if self.nodes.kind(specifier) != SyntaxKind::StringLiteral {
            return false;
        }
        match self.nodes.kind(parent) {
            SyntaxKind::SourceFile => true,
            SyntaxKind::ModuleBlock => self.nodes.parent(parent).is_some_and(|module| {
                matches!(self.node_map.get(module),
                    Some(Node::ModuleDeclaration(m)) if matches!(m.name, Some(tsr_ast::ModuleName::StringLiteral(_))))
            }),
            _ => false,
        }
    }

    /// TS1294 — `This syntax is not allowed when 'erasableSyntaxOnly' is
    /// enabled.` Each arm is one `shouldCheckErasableSyntax` site:
    /// a parameter property (`checkParameter`, `checker.go:2667`), a
    /// non-ambient enum (`checkEnumDeclaration`, `:5076`), a non-ambient
    /// instantiated namespace (`checkModuleDeclaration`, `:5165`), a
    /// non-ambient `import =` (`checkImportEqualsDeclaration`, `:5469`), a
    /// non-ambient `export =` (`checkExportAssignment`, `:5595`) and an
    /// angle-bracket assertion (`checkAssertion`, `:12293`). Not in a JS file.
    fn check_erasable_syntax(&mut self, node: NodeId, typed: Node<'_>, ambient: bool) {
        let message = &messages::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED;
        let reports = match typed {
            Node::ParameterDeclaration(parameter) => parameter.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token) if matches!(
                    token.kind,
                    SyntaxKind::PublicKeyword
                        | SyntaxKind::PrivateKeyword
                        | SyntaxKind::ProtectedKeyword
                        | SyntaxKind::ReadonlyKeyword
                        | SyntaxKind::OverrideKeyword
                ))
            }),
            Node::EnumDeclaration(_) | Node::ImportEqualsDeclaration(_) => !ambient,
            Node::ExportAssignment(assignment) => assignment.is_export_equals && !ambient,
            Node::ModuleDeclaration(_) => {
                !ambient && self.module_declaration_is_instantiated_or_preserved(node)
            }
            Node::TypeAssertion(assertion) => {
                if self.in_js_file(node) {
                    return;
                }
                // `[SkipTrivia(node.Pos()), node.Expression().Pos())`: the
                // `<T>` and the trivia after it, up to the operand's token.
                let Some(expression) = assertion.expression.and_then(|e| e.node_id()) else {
                    return;
                };
                let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
                let start = self.nodes.span(node).start;
                let end = self.full_start_before(file, expression);
                self.report(file, Diagnostic::new(message, tsr_core::Span::new(start, end)));
                return;
            }
            _ => false,
        };
        if !reports || self.in_js_file(node) {
            return;
        }
        self.error_on_node(node, message, Vec::new());
    }

    /// `isInstantiatedModule(node, ShouldPreserveConstEnums())` and the
    /// value-module symbol test that guards it (`checker.go:5164`).
    fn module_declaration_is_instantiated_or_preserved(&self, node: NodeId) -> bool {
        let Some(typed) = self.node_map.get(node) else { return false };
        let mut parents: Vec<_> =
            self.nodes.ancestors(node).filter_map(|ancestor| self.node_map.get(ancestor)).collect();
        parents.reverse();
        match tsr_ast::module_instance_state(typed, &parents) {
            tsr_ast::ModuleInstanceState::Instantiated => true,
            tsr_ast::ModuleInstanceState::ConstEnumOnly => self.preserve_const_enums,
            tsr_ast::ModuleInstanceState::NonInstantiated => false,
        }
    }

    /// `node.Pos()` for a node whose span starts at its first token: the end
    /// of the previous token, found by stepping back over whitespace in the
    /// source (a comment between the two is not stepped over). Without
    /// source text, the token start.
    fn full_start_before(&self, file: NodeId, node: NodeId) -> u32 {
        let start = self.nodes.span(node).start;
        let Some(text) = self.module_host.and_then(|host| host.source_text(file, self.nodes))
        else {
            return start;
        };
        let Some(before) = text.get(..start as usize) else { return start };
        let trimmed = before.trim_end();
        u32::try_from(trimmed.len()).unwrap_or(start)
    }

    /// TS7029 — `checkCaseBlock`'s tail (`checker.go:4196`): under
    /// `noFallthroughCasesInSwitch`, a clause whose end the binder recorded
    /// as a fallthrough (`FallthroughFlowNode`, set for every clause but the
    /// last whose end is not syntactically unreachable) and whose flow node
    /// is reachable (`isReachableFlowNode`, the flow port's).
    ///
    /// `error(clause)` spans `GetErrorSpanForNode`'s clause arm: from the
    /// `case`/`default` keyword to the first statement's full start.
    fn check_fallthrough_case(&mut self, node: NodeId, clause: &tsr_ast::CaseOrDefaultClause<'_>) {
        if !self.module_format_options.no_fallthrough_cases_in_switch {
            return;
        }
        let Some(flow) = self.binder.fallthrough_flow(node) else { return };
        if !self.is_reachable_flow_node(flow) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        let end = clause
            .statements
            .first()
            .and_then(tsr_ast::Statement::node_id)
            .map_or(span.end, |first| self.full_start_before(file, first));
        self.report(
            file,
            Diagnostic::new(
                &messages::FALLTHROUGH_CASE_IN_SWITCH,
                tsr_core::Span::new(span.start, end.max(span.start)),
            ),
        );
    }
}
