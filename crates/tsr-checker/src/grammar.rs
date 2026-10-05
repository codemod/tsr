//! Grammar checks ported from typescript-go's `internal/checker/grammarchecks.go`
//! that the parser lane owns.
//!
//! Each one is a `grammarError*` report: it stands only in a file without
//! parse diagnostics, which the caller (`check_grammar_modifier_shapes`)
//! already guarantees.

use tsr_ast::{ModifierLike, Node, NodeId, ObjectLiteralElementLike, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The modifier arm of typescript-go's
    /// `Checker.checkGrammarObjectLiteralExpression` (`grammarchecks.go`):
    /// modifiers are never allowed on object literal members except `async`
    /// on a method, and each one is reported as TS1042.
    pub(crate) fn check_grammar_object_literal_modifiers(&mut self, typed: Node<'_>) {
        let Node::ObjectLiteralExpression(literal) = typed else { return };
        for property in literal.properties {
            let (modifiers, is_method) = match property {
                ObjectLiteralElementLike::MethodDeclaration(method) => (method.modifiers, true),
                ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    (accessor.modifiers, false)
                }
                ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    (accessor.modifiers, false)
                }
                ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    (assignment.modifiers, false)
                }
                ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    (shorthand.modifiers, false)
                }
                ObjectLiteralElementLike::SpreadAssignment(_) => continue,
            };
            for modifier in modifiers {
                let ModifierLike::Token(token) = modifier else { continue };
                if is_method && token.kind == SyntaxKind::AsyncKeyword {
                    continue;
                }
                let Some(id) = token.node_id else { continue };
                self.report_modifier_cannot_be_used_here(id, token.kind);
            }
        }
    }

    /// `grammarErrorOnNode(mod, X_0_modifier_cannot_be_used_here, …)`.
    fn report_modifier_cannot_be_used_here(&mut self, modifier: NodeId, kind: SyntaxKind) {
        let Some(file) = self.source_file_of_for_diagnostics(modifier) else { return };
        let span = self.error_span(modifier);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_MODIFIER_CANNOT_BE_USED_HERE,
                span,
                [modifier_text(kind).to_string()],
            ),
        );
    }
}

/// The source spelling of a modifier keyword (`scanner.GetTextOfNode` on a
/// modifier token, which is always exactly its keyword).
fn modifier_text(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::AbstractKeyword => "abstract",
        SyntaxKind::AccessorKeyword => "accessor",
        SyntaxKind::AsyncKeyword => "async",
        SyntaxKind::ConstKeyword => "const",
        SyntaxKind::DeclareKeyword => "declare",
        SyntaxKind::DefaultKeyword => "default",
        SyntaxKind::ExportKeyword => "export",
        SyntaxKind::InKeyword => "in",
        SyntaxKind::OutKeyword => "out",
        SyntaxKind::OverrideKeyword => "override",
        SyntaxKind::PrivateKeyword => "private",
        SyntaxKind::ProtectedKeyword => "protected",
        SyntaxKind::PublicKeyword => "public",
        SyntaxKind::ReadonlyKeyword => "readonly",
        SyntaxKind::StaticKeyword => "static",
        _ => "",
    }
}
