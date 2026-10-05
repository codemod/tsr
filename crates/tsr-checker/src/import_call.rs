//! `checkImportCallExpression`'s diagnostics (`checker.go:8267`).
//!
//! The call's type is `calls.rs`'s `check_import_call_expression`; this is the
//! specifier arm only, TS7036 at `checker.go:8285`. No cache, side table or
//! traversal: the one semantic query is the specifier's memoised expression
//! type and one assignability relation to `string`.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::relater::{Relation, Ternary};

impl Checker<'_, '_> {
    /// TS7036 — `Dynamic import's specifier must be of type 'string', but here
    /// has type '{0}'.`
    ///
    /// `checkImportCallExpression` (`checker.go:8267`): the first argument's
    /// `checkExpressionCached` type reports when it is `Nullable` by flag or
    /// not assignable to `string`. Only a definite `NotRelated` reports the
    /// relation leg, which is `assignreport.rs`'s rule for the same query; an
    /// uncomputed specifier (`errorType`, upstream's `any`) never reports.
    pub(crate) fn check_import_call_specifier(&mut self, node: NodeId) {
        let Some(Node::CallExpression(call)) = self.node_map.get(node) else { return };
        if !matches!(
            call.expression,
            Some(Expression::KeywordExpression(keyword))
                if keyword.kind == SyntaxKind::ImportKeyword
        ) {
            return;
        }
        let Some(&specifier) = call.arguments.first() else { return };
        let Some(at) = specifier.node_id() else { return };
        let specifier_type = self.check_expression(specifier);
        if self.is_error(specifier_type) {
            return;
        }
        if !self.store.get(specifier_type).flags.intersects(TypeFlags::NULLABLE) {
            let string = self.intrinsics.string;
            if self.relate_ternary(specifier_type, string, Relation::Assignable)
                != Ternary::NotRelated
            {
                return;
            }
        }
        let printed = self.type_to_string(specifier_type);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::DYNAMIC_IMPORT_S_SPECIFIER_MUST_BE_OF_TYPE_STRING_BUT_HERE_HAS_TYPE_0,
                span,
                [printed],
            ),
        );
    }
}
