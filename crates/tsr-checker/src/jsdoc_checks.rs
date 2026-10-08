//! The JSDoc type nodes upstream's `checkSourceElement` visits.
//!
//! typescript-go reparses a JS file's JSDoc into ordinary AST
//! (`internal/parser/reparser.go`): a typedef becomes a
//! `JSTypeAliasDeclaration` statement (`reparseUnhosted`), and a hosted
//! `@type`/`@param`/`@returns` becomes the `Type` of the declaration,
//! parameter or function it documents (`reparseHosted`). The checker then
//! walks those nodes like written ones — `checkVariableLikeDeclaration`,
//! `checkParameter`, `checkSignatureDeclaration` and
//! `checkTypeAliasDeclaration` each `checkSourceElement` their type — so every
//! type-node rule (TS2344's `checkTypeArgumentConstraints`, arity, …) runs on
//! them.
//!
//! This port keeps JSDoc in the `jsdoc_entries` side table
//! ([ADR-0046](../../../docs/adr/0046-jsdoc-reparse-is-a-checker-query.md)),
//! so `check_node`'s child walk never reaches a comment. This module answers,
//! for one node, the reparsed type nodes the native walk would reach through
//! it, and `check_node` visits each with the same walk. Each is the node the
//! typing road already reads for the same slot (`jsdoc_type_annotation`,
//! [`Checker::jsdoc_reparsed_parameter_type`], the replay's `return_type`), so
//! the checked node and the typed node cannot disagree.
//!
//! Not yet covered, each a reparsed node upstream also checks: casts
//! (`makeNewCast`), `@satisfies`, `@this`, a function's full-signature
//! `@type`, `@template` constraints, `@augments`/`@implements` heritage, a
//! `@callback` signature, `@overload` signatures and `@import` declarations;
//! and hosted `@type` on export and accessor hosts. A class property's own
//! `@type` ([`Checker::jsdoc_self_hosted_type`]) is visited.
//! See `docs/parity/notes/r5-jsdoc2.md` §2 and `r5-jsdoc3.md` §3.
//!
//! No cache or side table: one `jsdoc_entries` probe per node, and for a
//! parameter or function the replay [`Checker::jsdoc_reparsed_function`]
//! answers from hash lookups when no comment applies.

use tsr_ast::{JSDocTag, Node, NodeId, SyntaxKind, TypeNode};

use crate::checker::Checker;
use crate::jsdoc_params::top_level_tags;

impl<'a> Checker<'a, '_> {
    /// The reparsed type nodes `checkSourceElement` reaches through `node`,
    /// in upstream's order: a declaration's own type, then (for the comment
    /// `node` hosts) its typedef aliases, which `reparseList` appends after
    /// the host statement.
    pub(crate) fn jsdoc_reparsed_type_nodes(&self, node: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let kind = self.nodes.kind(node);
        let hosts_reparse = matches!(
            kind,
            SyntaxKind::VariableDeclaration
                | SyntaxKind::Parameter
                | SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
        );
        let docs = self.jsdoc_entries.get(&node);
        if !hosts_reparse && docs.is_none() {
            return out;
        }
        if !self.in_js_file(node) {
            return out;
        }
        let mut push = |ty: Option<TypeNode<'a>>| {
            if let Some(id) = ty.and_then(|ty| Node::from(ty).node_id()) {
                out.push(id);
            }
        };
        match self.node_map.get(node) {
            // `checkVariableLikeDeclaration` → `checkSourceElement(node.Type())`.
            Some(Node::VariableDeclaration(declaration)) if declaration.r#type.is_none() => {
                push(self.jsdoc_type_annotation(node));
            }
            // `checkPropertyDeclaration` → `checkVariableLikeDeclaration`.
            Some(Node::PropertyDeclaration(property)) if property.r#type.is_none() => {
                push(self.jsdoc_self_hosted_type(node));
            }
            // `checkParameter` → `checkVariableLikeDeclaration`.
            Some(Node::ParameterDeclaration(parameter)) if parameter.r#type.is_none() => {
                push(self.jsdoc_reparsed_parameter_type(node));
            }
            // `checkSignatureDeclaration` → `checkSourceElement(node.Type())`.
            Some(_)
                if hosts_reparse
                    && kind != SyntaxKind::VariableDeclaration
                    && self.function_like_parts(node).is_some_and(|parts| !parts.return_type) =>
            {
                let function = self.jsdoc_reparsed_function(node);
                if function.full_signature.is_none() {
                    push(function.return_type);
                }
            }
            _ => {}
        }
        // `reparseUnhosted`'s typedef arm: every comment, not only the last.
        for doc in docs.copied().unwrap_or_default() {
            for tag in top_level_tags(doc.tags) {
                if let JSDocTag::JSDocTypedefTag(typedef) = tag
                    && let Some(Node::JSDocTypeExpression(expression)) = typedef.type_expression
                {
                    push(expression.r#type);
                }
            }
        }
        out
    }
}
