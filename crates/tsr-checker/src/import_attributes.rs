//! `checkImportAttributes` (`checker.go:5408`): the grammar of a
//! `with { … }` / `assert { … }` clause on an import or export declaration,
//! by `--module` and by the declaration's emit syntax.
//!
//! Called once per declaration from the check walk's `ImportDeclaration` and
//! `ExportDeclaration` arms, which is where upstream's `checkImportDeclaration`
//! (`:5330`) and `checkExportDeclaration` (`:5548`) end. Each grammar report is a
//! `grammarErrorOnNode`, so none is reported in a file with parse
//! diagnostics. No cache, side table or traversal: the reads are the
//! declaration's own syntax and the host's
//! `GetEmitSyntaxForUsageLocation` answer for its specifier.
//!
//! The opening relation check, `checkTypeAssignableTo(getTypeFromImportAttributes(node),
//! getNullableType(ImportAttributes, Undefined))`, is
//! [`Checker::check_import_attributes_assignable`] in
//! `import_attribute_checks.rs`, called here before the parse-error gate
//! because it is a `c.error`, not a grammar report
//! (`docs/parity/notes/r6-triage.md` §4; formerly r5-modules.md §3).

use tsr_ast::{ImportAttributeName, ImportAttributes, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Message, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `checkImportAttributes` (`checker.go:5408`) for an import or export
    /// declaration. A declaration outside a source file, module block or
    /// module declaration is the `checkGrammarModuleElementContext` bail at
    /// the top of both callers, which never reach this check.
    pub(crate) fn check_import_attributes(&mut self, node: NodeId) {
        use tsr_core::ModuleKind;
        let (attributes, specifier, is_type_only) = match self.node_map.get(node) {
            Some(Node::ImportDeclaration(declaration)) => (
                declaration.attributes,
                declaration.module_specifier,
                declaration.import_clause.is_some_and(|clause| {
                    clause.phase_modifier.is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
                }),
            ),
            Some(Node::ExportDeclaration(declaration)) => {
                (declaration.attributes, declaration.module_specifier, declaration.is_type_only)
            }
            _ => return,
        };
        let Some(attributes) = attributes else { return };
        let Some(attributes_id) = attributes.node_id else { return };
        if !self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::SourceFile | SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration
            )
        }) {
            return;
        }
        // The relation opening `checkImportAttributes` is a `c.error`, so it
        // stands in a file with parse errors; the grammar arms below do not.
        self.check_import_attributes_assignable(node);
        if self.file_has_parse_errors {
            return;
        }
        let has_override = self.resolution_mode_override(attributes, is_type_only);
        if is_type_only && has_override {
            // "Other grammar checks do not apply to type-only imports with
            // resolution mode attributes."
            return;
        }
        // `ModuleKind.SupportsImportAttributes` (`core/compileroptions.go:415`).
        let supports = (ModuleKind::Node18..=ModuleKind::NodeNext).contains(&self.module_kind)
            || matches!(self.module_kind, ModuleKind::Preserve | ModuleKind::ESNext);
        if !supports {
            self.grammar_error_on_node(
                attributes_id,
                &messages::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NODE18_NODE20_NODENEXT_OR_PRESERVE,
            );
            return;
        }
        if let Some(specifier) = specifier.and_then(|specifier| specifier.node_id())
            && self.module_specifier_emit_syntax(specifier) == ModuleKind::CommonJS
        {
            self.grammar_error_on_node(
                attributes_id,
                &messages::IMPORT_ATTRIBUTES_ARE_NOT_ALLOWED_ON_STATEMENTS_THAT_COMPILE_TO_COMMONJS_REQUIRE_CALLS,
            );
            return;
        }
        if is_type_only {
            self.grammar_error_on_node(
                attributes_id,
                &messages::IMPORT_ATTRIBUTES_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS,
            );
            return;
        }
        if has_override {
            self.grammar_error_on_node(
                attributes_id,
                &messages::RESOLUTION_MODE_CAN_ONLY_BE_SET_FOR_TYPE_ONLY_IMPORTS,
            );
        }
    }

    /// `checkImportType`'s attributes step (`checker.go:3327`):
    /// `getResolutionModeOverride(attributes, reportErrors=true)`, for its
    /// grammar errors only.
    pub(crate) fn check_import_type_attributes(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ImportTypeNode(import)) = self.node_map.get(node) else { return };
        if let Some(attributes) = import.attributes {
            self.resolution_mode_override(attributes, true);
        }
    }

    /// `getResolutionModeOverride` (`checker.go:3332`), answering whether it
    /// returns a mode other than `ResolutionModeNone`; with `report_errors`
    /// its three grammar errors (TS1464, TS1463, TS1453).
    fn resolution_mode_override(
        &mut self,
        attributes: &ImportAttributes<'_>,
        report_errors: bool,
    ) -> bool {
        let report = |checker: &mut Self, at: Option<NodeId>, message: &'static Message| {
            if report_errors && let Some(at) = at {
                checker.grammar_error_on_node(at, message);
            }
        };
        let [element] = attributes.attributes else {
            report(
                self,
                attributes.node_id,
                &messages::TYPE_IMPORT_ATTRIBUTES_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IMPORT_OR_REQUIRE,
            );
            return false;
        };
        let Some(ImportAttributeName::StringLiteral(name)) = element.name else { return false };
        if name.text != "resolution-mode" {
            report(
                self,
                name.node_id,
                &messages::RESOLUTION_MODE_IS_THE_ONLY_VALID_KEY_FOR_TYPE_IMPORT_ATTRIBUTES,
            );
            return false;
        }
        let value = match element.value.map(Node::from) {
            Some(Node::StringLiteral(literal)) => (literal.text, literal.node_id),
            Some(Node::NoSubstitutionTemplateLiteral(literal)) => (literal.text, literal.node_id),
            _ => return false,
        };
        if value.0 != "import" && value.0 != "require" {
            report(self, value.1, &messages::RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT);
            return false;
        }
        true
    }

    /// `getEmitSyntaxForModuleSpecifierExpression` (`checker.go:14877`):
    /// `ModuleKindNone` unless the specifier is a string literal.
    fn module_specifier_emit_syntax(&self, usage: NodeId) -> tsr_core::ModuleKind {
        if !matches!(
            self.nodes.kind(usage),
            SyntaxKind::StringLiteral | SyntaxKind::NoSubstitutionTemplateLiteral
        ) {
            return tsr_core::ModuleKind::None;
        }
        let (Some(host), Some(importing)) = (self.module_host, self.source_file_of(usage)) else {
            return tsr_core::ModuleKind::None;
        };
        host.emit_syntax_for_usage_location(importing, usage)
    }
}
