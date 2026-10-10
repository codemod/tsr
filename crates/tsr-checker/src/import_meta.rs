//! `checkMetaProperty`'s `import` arm (`checker.go:10753`): the
//! `import.meta` type and its module-kind errors, with the grammar check
//! `checkGrammarMetaProperty` (`grammarchecks.go:1831`) that opens it.
//!
//! Upstream runs all three from `checkExpression`, so the diagnostics are a
//! side effect of the first type query. Here the type answer
//! ([`Checker::check_meta_property_type`]) is the query road's arm in
//! `check_expression_worker` and reports nothing, and the reports
//! ([`Checker::check_meta_property_reports`]) run from the check walk's
//! `MetaProperty` visit, beside `checkNewTargetMetaProperty`'s TS17013, which
//! that walk already owns. No cache, side table or traversal: the only
//! semantic read is the global `ImportMeta` declared type, itself cached by
//! `get_declared_type_of_symbol`. See `docs/parity/notes/r5-modules.md` §2.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::types::TypeId;

impl Checker<'_, '_> {
    /// `getGlobalImportMetaType` (`checker.go:1064`):
    /// `getGlobalType("ImportMeta", 0, reportErrors=true)`, which answers
    /// `emptyObjectType` when the lib has no such interface.
    pub(crate) fn global_import_meta_type(&mut self) -> TypeId {
        self.global_type_symbol_with_arity("ImportMeta", 0)
            .map_or(self.intrinsics.empty_object, |symbol| self.get_declared_type_of_symbol(symbol))
    }

    /// The type half of `checkMetaProperty` (`checker.go:10753`):
    /// `checkImportMetaProperty` (`:10782`) answers the global `ImportMeta`
    /// for `import.meta` and `errorType` for any other name, `import.defer`
    /// is `errorType` too, and `new.target` is
    /// [`Checker::check_new_target_meta_property_type`].
    pub(crate) fn check_meta_property_type(&mut self, node: &tsr_ast::MetaProperty<'_>) -> TypeId {
        if node.keyword_token.kind == SyntaxKind::NewKeyword {
            return match node.node_id {
                Some(id) => self.check_new_target_meta_property_type(id),
                None => self.intrinsics.error,
            };
        }
        if node.keyword_token.kind != SyntaxKind::ImportKeyword {
            return self.intrinsics.error;
        }
        match node.name {
            Some(name) if name.text == "meta" => self.global_import_meta_type(),
            _ => self.intrinsics.native_error,
        }
    }

    /// The type half of `checkNewTargetMetaProperty` (`checker.go:10768`):
    /// `GetNewTargetContainer` (`ast/utilities.go`) is the `this` container
    /// when it is a constructor or a function declaration or expression. No
    /// container is reported (TS17013, `check_new_target_meta_property`)
    /// and answers `errorType`; a constructor answers its class symbol's
    /// type, a function its own symbol's.
    fn check_new_target_meta_property_type(&mut self, node: NodeId) -> TypeId {
        let Some(container) = self.new_target_this_container(node).filter(|&container| {
            matches!(
                self.nodes.kind(container),
                SyntaxKind::Constructor
                    | SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
            )
        }) else {
            return self.intrinsics.native_error;
        };
        let declaration = if self.nodes.kind(container) == SyntaxKind::Constructor {
            match self.nodes.parent(container) {
                Some(class) => class,
                None => return self.intrinsics.error,
            }
        } else {
            container
        };
        match self.binder.symbol_of(declaration) {
            Some(symbol) => self.get_type_of_symbol(symbol),
            None => self.intrinsics.error,
        }
    }

    /// The reporting half of `checkMetaProperty` for the check walk:
    /// `checkGrammarMetaProperty` (`grammarchecks.go:1831`) for both
    /// keywords, then `checkImportMetaProperty`'s module-kind errors
    /// (`checker.go:10782`) for `import.<name>` other than `import.defer`.
    /// `new.target`'s TS17013 stays with `check_new_target_meta_property`.
    pub(crate) fn check_meta_property_reports(&mut self, node: NodeId) {
        let Some(Node::MetaProperty(meta)) = self.node_map.get(node) else { return };
        let keyword = meta.keyword_token.kind;
        let Some(name) = meta.name else { return };
        let name_text = name.text;
        if !self.file_has_parse_errors {
            self.check_grammar_meta_property(node, keyword, name, name_text);
        }
        if keyword != SyntaxKind::ImportKeyword || name_text == "defer" {
            return;
        }
        self.check_import_meta_property_module_kind(node);
    }

    /// `checkGrammarMetaProperty` (`grammarchecks.go:1831`). A
    /// `grammarErrorOnNode`/`grammarErrorAtPos` report, so the caller has
    /// already required a file without parse diagnostics.
    fn check_grammar_meta_property(
        &mut self,
        node: NodeId,
        keyword: SyntaxKind,
        name: &tsr_ast::Identifier<'_>,
        name_text: &str,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let (keyword_text, expected) = match keyword {
            SyntaxKind::NewKeyword if name_text != "target" => ("new", "target"),
            SyntaxKind::ImportKeyword if name_text != "meta" => ("import", "meta"),
            _ => return,
        };
        let is_callee = self.nodes.parent(node).is_some_and(|parent| {
            matches!(self.node_map.get(parent), Some(Node::CallExpression(call))
                if call.expression.and_then(|expression| expression.node_id()) == Some(node))
        });
        if keyword == SyntaxKind::ImportKeyword && name_text == "defer" {
            if !is_callee {
                // `grammarErrorAtPos(node, node.End(), 0, X_0_expected, "(")`.
                let end = self.nodes.span(node).end;
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::_0_EXPECTED,
                        tsr_core::Span::new(end, end),
                        ["(".to_string()],
                    ),
                );
            }
            return;
        }
        let Some(name_id) = name.node_id else { return };
        let span = self.error_span(name_id);
        let diagnostic = if keyword == SyntaxKind::ImportKeyword && is_callee {
            Diagnostic::with_args(
                &messages::_0_IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_IMPORT_DID_YOU_MEAN_META_OR_DEFER,
                span,
                [name_text.to_string()],
            )
        } else {
            Diagnostic::with_args(
                &messages::_0_IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_1_DID_YOU_MEAN_2,
                span,
                [name_text.to_string(), keyword_text.to_string(), expected.to_string()],
            )
        };
        self.report(file, diagnostic);
    }

    /// `checkImportMetaProperty`'s module-kind arms (`checker.go:10782`):
    /// under `node16`..`nodenext` a file whose implied format is not ESM is
    /// TS1470; below `es2020` (other than `system`) every use is TS1343.
    ///
    /// The implied format is read through
    /// `ModuleHost::implied_node_format_for_emit`, which is
    /// `GetImpliedNodeFormatForEmitWorker`: under an emit module kind in
    /// `node16`..`nodenext` — the only arm that reads it — that worker
    /// returns `sourceFileMetaData.ImpliedNodeFormat` unchanged
    /// (`ast/utilities.go:2574`).
    fn check_import_meta_property_module_kind(&mut self, node: NodeId) {
        use tsr_core::ModuleKind;
        let message = if (ModuleKind::Node16..=ModuleKind::NodeNext).contains(&self.module_kind) {
            let Some(file) = self.source_file_of(node) else { return };
            let implied = self
                .module_host
                .map_or(ModuleKind::None, |host| host.implied_node_format_for_emit(file));
            if implied == ModuleKind::ESNext {
                return;
            }
            &messages::THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMMONJS_OUTPUT
        } else if self.module_kind < ModuleKind::ES2020 && self.module_kind != ModuleKind::System {
            &messages::THE_IMPORT_META_META_PROPERTY_IS_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_ES2020_ES2022_ESNEXT_SYSTEM_NODE16_NODE18_NODE20_OR_NODENEXT
        } else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::new(message, span));
    }
}
