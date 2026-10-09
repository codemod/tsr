//! TS2532 / TS18048 and their `null` twins on the left of a qualified name in
//! a type query: `typeof foo.a.b.c` where `foo.a.b` may be `undefined`.
//!
//! `checkQualifiedName` (`checker.go:8122`) checks its left side through
//! `checkNonNullExpression` (`checkNonNullType` with the left as the reported
//! node), or for a `this` head in a type query through
//! `checkNonNullType(checkThisExpression(left), left)`. The left of an inner
//! qualified name is checked first, because checking the left *is* checking
//! the inner qualified name. `getTypeFromTypeQueryNode` reaches this through
//! `checkExpressionWithTypeArguments`, once per query node (its resolved type
//! is cached in the node links).
//!
//! The type side was already ported: [`Checker::check_qualified_name`]
//! (`members.rs`) strips the left with `check_non_null_type`, with no
//! reporter, because the check walk owns diagnostics. This file is that
//! walk's half for a `TypeQuery` node. The reporter is the receivers' one,
//! `reportObjectPossiblyNullOrUndefinedError` (`checker.go:7455`): an
//! identifier left is an entity name expression and names itself; a
//! qualified-name left is not an expression, so it takes the `Object is
//! possibly …` arm.
//!
//! Not ported: the `unknown` arm (TS18046/TS2571) of `checkNonNullType`,
//! declined for every receiver in this port (`nullable_operand.rs`).
//!
//! No cache or side table. The left types are the ones
//! `get_type_from_type_query_node` computes, through the same calls.
//!
//! `docs/parity/notes/r6-smallcodes5.md` §2.2.

use tsr_ast::{EntityName, Expression, Node, NodeId, QualifiedName};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

#[expect(dead_code, reason = "r6-smallcodes5 hook diff not applied")]
impl Checker<'_, '_> {
    /// The receiver reports of one `TypeQuery` node's entity name.
    pub(crate) fn check_type_query_nullable_receivers(&mut self, node: NodeId) {
        let Some(Node::TypeQueryNode(query)) = self.node_map.get(node) else { return };
        if let Some(EntityName::QualifiedName(qualified)) = query.expr_name {
            self.check_qualified_name_left_non_null(qualified);
        }
    }

    /// `checkQualifiedName`'s left arm, innermost first.
    fn check_qualified_name_left_non_null(&mut self, node: &QualifiedName<'_>) {
        let Some(left) = node.left else { return };
        let left_type = match left {
            EntityName::QualifiedName(inner) => {
                self.check_qualified_name_left_non_null(inner);
                self.check_qualified_name(inner)
            }
            EntityName::Identifier(identifier) if identifier.text == "this" => {
                let Some(id) = identifier.node_id else { return };
                self.check_this_expression(id)
            }
            EntityName::Identifier(identifier) => {
                self.check_expression(Expression::Identifier(identifier))
            }
        };
        if self.is_error(left_type) {
            return;
        }
        match left {
            EntityName::Identifier(identifier) => {
                self.report_nullable_operand_of_type(Expression::Identifier(identifier), left_type);
            }
            EntityName::QualifiedName(inner) => {
                let Some(id) = inner.node_id else { return };
                let message = match self.nullish_facts(left_type) {
                    (true, true) => &messages::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                    (false, true) => &messages::OBJECT_IS_POSSIBLY_UNDEFINED,
                    (true, false) => &messages::OBJECT_IS_POSSIBLY_NULL,
                    (false, false) => return,
                };
                let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
                let span = self.error_span(id);
                self.report(file, Diagnostic::new(message, span));
            }
        }
    }
}
