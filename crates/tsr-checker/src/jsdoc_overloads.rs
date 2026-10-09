//! `@overload` signatures: the declarations `reparseUnhosted`'s
//! `KindJSDocOverloadTag` arm (`parser/reparser.go:134`) emits for a JS
//! function, method or constructor.
//!
//! Native's reparser turns each `@overload` tag of every comment on such a
//! host into a body-less `FunctionDeclaration` / `MethodDeclaration` /
//! `ConstructorDeclaration` (`reparseJSDocSignature`, `:142`) with the host's
//! name and modifiers, the comment's `@template` parameters
//! (`gatherTypeParameters(jsDoc, false)`, `:293`), the run's reparsed
//! parameters and its `@return` type; `parseListIndex`
//! (`parser/parser.go:619`) puts it *before* the host, and the binder makes it
//! another declaration of the host's symbol. `getSignaturesOfSymbol`
//! (`checker.go:19806`) then reads it like a written overload, and skips the
//! host as the implementation (`previous.Flags&NodeFlagsReparsed != 0`).
//!
//! This port has no reparser ([ADR-0046]). The parser stores the run as the
//! tag's function type (`parse_jsdoc_signature`, as for `@callback`), whose
//! parameters are real reparsed `ParameterDeclaration`s the binder binds; the
//! binder files the **tag** in the host symbol's declarations, ahead of the
//! host. This module answers the parts of the reparsed declaration the
//! function type cannot carry — its host, kind and type parameters — and
//! builds its signature from the function type through
//! `get_signature_from_declaration`, so the parameters, `this`, `?` and `...`
//! are read exactly as a written signature's.
//!
//! No cache or side table: every query is a parent walk to the comment, one
//! `jsdoc_hosts` lookup and the comment's tags. The signature itself is built
//! where the caller builds every other declaration's
//! (`getSignaturesOfSymbol` and the class's constructor list), and memoized
//! there with the symbol's type.
//!
//! [ADR-0046]: ../../../docs/adr/0046-jsdoc-reparse-is-a-checker-query.md

use tsr_ast::{JSDocTag, Node, NodeId, SyntaxKind, TypeNode};
use tsr_core::Span;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl<'a> Checker<'a, '_> {
    /// The host whose comment holds `tag`, when `tag` is an `@overload` that
    /// `reparseUnhosted` makes a declaration of (a function, method or
    /// constructor declaration outside every object literal).
    #[expect(
        dead_code,
        reason = "called from the hooks in docs/parity/notes/r6-jsdoc-overload.diff"
    )]
    pub(crate) fn jsdoc_overload_host(&self, tag: NodeId) -> Option<NodeId> {
        if self.nodes.kind(tag) != SyntaxKind::JSDocOverloadTag {
            return None;
        }
        let mut root = tag;
        while let Some(parent) = self.nodes.parent(root) {
            root = parent;
        }
        let host = *self.jsdoc_hosts.get(&root)?;
        self.jsdoc_hosts_overloads(host).then_some(host)
    }

    /// The `@overload` tags whose declarations `reparseUnhosted` puts before
    /// `host`, in source order: every comment's, not only the last one's.
    #[expect(
        dead_code,
        reason = "called from the hooks in docs/parity/notes/r6-jsdoc-overload.diff"
    )]
    pub(crate) fn jsdoc_overload_tags(&self, host: NodeId) -> Vec<NodeId> {
        let Some(docs) = self.jsdoc_entries.get(&host) else { return Vec::new() };
        if !self.jsdoc_hosts_overloads(host) || !self.in_js_file(host) {
            return Vec::new();
        }
        docs.iter()
            .flat_map(|doc| doc.tags.iter())
            .filter_map(|tag| match tag {
                JSDocTag::JSDocOverloadTag(overload) => overload.node_id,
                _ => None,
            })
            .collect()
    }

    /// `getSignaturesOfSymbol`'s implementation rule against a reparsed
    /// overload (`checker.go:19818`): `declaration` has a body, and
    /// `previous` is an `@overload` declaration whose host shares its kind
    /// and parent — upstream's `decl.Parent == previous.Parent && decl.Kind ==
    /// previous.Kind`, with the reparsed flag standing in for adjacency.
    /// `None` when `previous` is not an `@overload` declaration.
    #[expect(
        dead_code,
        reason = "called from the hooks in docs/parity/notes/r6-jsdoc-overload.diff"
    )]
    pub(crate) fn jsdoc_overload_precedes_implementation(
        &self,
        declaration: NodeId,
        previous: NodeId,
    ) -> Option<bool> {
        let host = self.jsdoc_overload_host(previous)?;
        let has_body = match self.node_map.get(declaration) {
            Some(Node::FunctionDeclaration(node)) => node.body.is_some(),
            Some(Node::MethodDeclaration(node)) => node.body.is_some(),
            Some(Node::ConstructorDeclaration(node)) => node.body.is_some(),
            _ => false,
        };
        Some(
            has_body
                && self.nodes.kind(host) == self.nodes.kind(declaration)
                && self.nodes.parent(host) == self.nodes.parent(declaration),
        )
    }

    /// Where an error on the reparsed declaration lands:
    /// `reparseJSDocSignature` finishes it at the tag's name
    /// (`finishReparsedNode(signature, tag.TagName())`).
    #[expect(
        dead_code,
        reason = "called from the hooks in docs/parity/notes/r6-jsdoc-overload.diff"
    )]
    pub(crate) fn jsdoc_overload_error_span(&self, tag: NodeId) -> Option<Span> {
        self.jsdoc_overload_host(tag)?;
        let Some(Node::JSDocOverloadTag(overload)) = self.node_map.get(tag) else { return None };
        Some(self.nodes.span(overload.tag_name.node_id?))
    }

    /// The checks `checkSourceElement` runs on the declarations
    /// `reparseUnhosted` puts before `host` that the host's own check does
    /// not: `checkFunctionOrMethodDeclaration`'s last block
    /// (`checker.go:3446`) — a body-less declaration without a return type
    /// reports implicit any, which `reportImplicitAny` words as TS7012 for a
    /// reparsed one (`checker.go:18332`). The signature-set checks run once
    /// per symbol from the host (`check_function_or_constructor_symbol`).
    #[expect(
        dead_code,
        reason = "called from the hooks in docs/parity/notes/r6-jsdoc-overload.diff"
    )]
    pub(crate) fn check_jsdoc_overload_declarations(&mut self, host: NodeId) {
        if !self.no_implicit_any {
            return;
        }
        for tag in self.jsdoc_overload_tags(host) {
            let Some(Node::JSDocOverloadTag(overload)) = self.node_map.get(tag) else { continue };
            let Some(TypeNode::FunctionTypeNode(signature)) = overload.type_expression else {
                continue;
            };
            if signature.r#type.is_some() || self.nodes.kind(host) == SyntaxKind::Constructor {
                continue;
            }
            let (Some(span), Some(file)) =
                (self.jsdoc_overload_error_span(tag), self.source_file_of_for_diagnostics(host))
            else {
                continue;
            };
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::THIS_OVERLOAD_IMPLICITLY_RETURNS_THE_TYPE_0_BECAUSE_IT_LACKS_A_RETURN_TYPE_ANNOTATION,
                    span,
                    ["any".to_string()],
                ),
            );
        }
    }

    /// Whether `reparseUnhosted`'s `KindJSDocOverloadTag` arm
    /// (`parser/reparser.go:134`) makes signatures from `host`'s comments:
    /// a function, method or constructor declaration parsed outside every
    /// object literal's member list (`parsingContexts` keeps the
    /// `PCObjectLiteralMembers` bit through every list nested in one).
    pub(crate) fn jsdoc_hosts_overloads(&self, host: NodeId) -> bool {
        matches!(
            self.nodes.kind(host),
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
        ) && !self
            .nodes
            .ancestors(host)
            .any(|ancestor| self.nodes.kind(ancestor) == SyntaxKind::ObjectLiteralExpression)
    }
}
