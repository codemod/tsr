//! TS2783 — `'{0}' is specified more than once, so this usage will be
//! overwritten.`
//!
//! `checkSpreadPropOverrides` (`checker.go:13371`). Only the syntactic slice is
//! ported: a spread of an **identifier** whose declaration carries a written
//! **type literal**. Upstream's other two guards — `CheckFlagsPartial` and a
//! union operand — are satisfied by that restriction rather than by a test.
//!
//! `docs/architecture/checker-notes-diag2.md` §962.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// One object literal.
    pub(crate) fn check_spread_property_overrides(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else {
            return;
        };
        // Names of the property assignments seen so far, with the node upstream
        // reports on — `left.ValueDeclaration`, the assignment itself.
        let mut seen: Vec<(String, NodeId)> = Vec::new();
        let mut reports: Vec<(NodeId, String)> = Vec::new();
        for property in literal.properties {
            let Some(at) = property.node_id() else { continue };
            match self.node_map.get(at) {
                Some(Node::PropertyAssignment(assignment)) => {
                    if let tsr_ast::PropertyName::Identifier(name) = assignment.name {
                        seen.push((name.text.to_string(), at));
                    }
                }
                Some(Node::SpreadAssignment(spread)) => {
                    let Some(operand) = spread.expression.and_then(|e| e.node_id()) else {
                        continue;
                    };
                    for required in self.required_members_of_annotated_identifier(operand) {
                        // **Every earlier assignment of that name**, which is
                        // upstream's `props[right.Name]` after the fold has
                        // already collapsed duplicates — this port keeps the
                        // list and reports each, matching the corpus.
                        for (name, assignment) in &seen {
                            if *name == required {
                                reports.push((*assignment, required.clone()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for (at, name) in reports {
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                    span,
                    [name],
                ),
            );
        }
    }

    /// The **required** member names of `x` where `x` was declared with a
    /// written type literal — `declare let a: { a: string }`.
    ///
    /// A `?` excludes the member; `| undefined` does **not**, which is the
    /// difference `spreadDuplicate` tests with `c` against `b`.
    fn required_members_of_annotated_identifier(&mut self, operand: NodeId) -> Vec<String> {
        if self.nodes.kind(operand) != SyntaxKind::Identifier {
            return Vec::new();
        }
        let Some(text) = self.identifier_text(operand).map(str::to_string) else {
            return Vec::new();
        };
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, operand, &text, SymbolFlags::VALUE)
        else {
            return Vec::new();
        };
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        let [declaration] = declarations.as_slice() else { return Vec::new() };
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(*declaration) else {
            return Vec::new();
        };
        let Some(annotation) = variable.r#type.and_then(|t| t.node_id()) else { return Vec::new() };
        let Some(Node::TypeLiteralNode(literal)) = self.node_map.get(annotation) else {
            return Vec::new();
        };
        literal
            .members
            .iter()
            .filter_map(|member| {
                let id = member.node_id()?;
                let Some(Node::PropertySignatureDeclaration(signature)) = self.node_map.get(id)
                else {
                    return None;
                };
                // `?` is the `postfix_token` on a property signature.
                if signature
                    .postfix_token
                    .is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
                {
                    return None;
                }
                match signature.name {
                    tsr_ast::PropertyName::Identifier(name) => Some(name.text.to_string()),
                    _ => None,
                }
            })
            .collect()
    }
}
