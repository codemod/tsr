//! Initializers checked against a JSDoc `@type` annotation in a JS file.
//!
//! Upstream's `reparseHosted` (`parser/reparser.go`, `KindJSDocTypeTag`) copies
//! a `@type` tag onto the first untyped declaration of a variable statement, or
//! onto a hosting variable or property declaration, as its written type. From
//! then on `checkVariableLikeDeclaration` treats it exactly like a TypeScript
//! annotation: the initializer's type is checked with
//! `checkTypeAssignableToAndOptionallyElaborate` against the declared type.
//!
//! This port keeps JSDoc in a side table, so the annotation is read through the
//! same readers the declared-type path already uses
//! ([`Checker::jsdoc_type_annotation`] for variables,
//! [`Checker::jsdoc_cast_annotation`] for a hosted class property), and the
//! report goes through the TypeScript path's own excess-property and
//! assignability reporters.

use tsr_ast::{Node, NodeId, SyntaxKind, TypeNode};

use crate::{checker::Checker, types::TypeId};

impl Checker<'_, '_> {
    /// `checkVariableLikeDeclaration`'s initializer check for a declaration
    /// whose type is a reparsed JSDoc `@type`.
    pub(crate) fn check_jsdoc_annotated_initializer(&mut self, node: NodeId, ambient: bool) {
        // Mirrors the TypeScript arms' declines; their source-completeness
        // certification under parse errors is not reused here, so a file with
        // parse errors declines outright.
        if ambient || self.file_has_parse_errors || !self.in_js_file(node) {
            return;
        }
        // `checkExportAssignment` (`checker.go:5662`) elaborates at the
        // expression itself; the variable-like checks at the declaration.
        let (annotation, initializer, export) = match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) if declaration.r#type.is_none() => {
                (self.jsdoc_type_annotation(node), declaration.initializer, false)
            }
            Some(Node::PropertyDeclaration(property)) if property.r#type.is_none() => {
                (self.jsdoc_cast_annotation(node), property.initializer, false)
            }
            // `reparseHosted`'s `KindJSDocTypeTag` arm types an export
            // assignment; `node.Type()` is then read for `KindExportAssignment`
            // (both `export default` and `export =`).
            Some(Node::ExportAssignment(assignment)) => {
                (self.jsdoc_cast_annotation(node), assignment.expression, true)
            }
            _ => return,
        };
        let (Some(annotation), Some(initializer)) = (annotation, initializer) else { return };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(initializer);
        let Some(initializer_id) = initializer.node_id() else { return };
        let at = if export { initializer_id } else { node };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, initializer_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(at, initializer_id, source, target);
    }
}

impl<'a> Checker<'a, '_> {
    /// The `@satisfies` tag whose reparsed `SatisfiesExpression` is
    /// `expression`'s parent upstream: `reparseHosted`'s `KindJSDocSatisfiesTag`
    /// arm (`parser/reparser.go:396`) wraps the host's initializer or
    /// expression in `makeNewCast(type, expr, isAssertion=false)`. The
    /// innermost wrapper wins (a declaration's own comment is reparsed before
    /// its statement's), which is the one `getContextualType` meets first.
    pub(crate) fn jsdoc_satisfies_tag_of(
        &self,
        expression: NodeId,
    ) -> Option<&'a tsr_ast::JSDocSatisfiesTag<'a>> {
        let parent = self.nodes.parent(expression)?;
        let is =
            |e: Option<tsr_ast::Expression<'_>>| e.and_then(|e| e.node_id()) == Some(expression);
        let host = match self.node_map.get(parent)? {
            Node::ParenthesizedExpression(node) if is(node.expression) => parent,
            Node::ReturnStatement(node) if is(node.expression) => parent,
            Node::ExportAssignment(node) if is(node.expression) => parent,
            Node::PropertyAssignment(node) if is(node.initializer) => parent,
            Node::PropertyDeclaration(node) if is(node.initializer) => parent,
            Node::ShorthandPropertyAssignment(node) if is(node.object_assignment_initializer) => {
                parent
            }
            Node::VariableDeclaration(node) if is(node.initializer) => {
                if let Some(tag) = self.last_jsdoc_satisfies_tag(parent) {
                    return self.in_js_file(parent).then_some(tag);
                }
                // `KindVariableStatement`: the first declaration with an
                // initializer.
                let list = self.nodes.parent(parent)?;
                let Some(Node::VariableDeclarationList(declarations)) = self.node_map.get(list)
                else {
                    return None;
                };
                let first = declarations
                    .declarations
                    .iter()
                    .find(|declaration| declaration.initializer.is_some())?;
                if first.node_id != Some(parent) {
                    return None;
                }
                let statement = self.nodes.parent(list)?;
                if self.nodes.kind(statement) != SyntaxKind::VariableStatement {
                    return None;
                }
                statement
            }
            Node::BinaryExpression(binary) if is(binary.right) => {
                let statement = self.nodes.parent(parent)?;
                if self.nodes.kind(statement) != SyntaxKind::ExpressionStatement
                    || !self.is_assignment_declaration(binary)
                {
                    return None;
                }
                statement
            }
            _ => return None,
        };
        let tag = self.last_jsdoc_satisfies_tag(host)?;
        self.in_js_file(host).then_some(tag)
    }

    /// `getContextualType`'s `KindSatisfiesExpression` arm
    /// (`checker.go:29384`) for a reparsed `@satisfies`: the tag's type.
    pub(crate) fn jsdoc_satisfies_contextual_type(&mut self, expression: NodeId) -> Option<TypeId> {
        let tag = self.jsdoc_satisfies_tag_of(expression)?;
        let annotation = jsdoc_type_expression_type(tag.type_expression?)?;
        Some(self.get_type_from_type_node(annotation))
    }

    /// The **first** `@satisfies` tag (with a type) of a host's last comment;
    /// hosted arms read only the last comment (`reparseTags`' `isLast`).
    fn last_jsdoc_satisfies_tag(&self, host: NodeId) -> Option<&'a tsr_ast::JSDocSatisfiesTag<'a>> {
        let doc = self.jsdoc_entries.get(&host)?.last()?;
        doc.tags.iter().find_map(|tag| match tag {
            tsr_ast::JSDocTag::JSDocSatisfiesTag(tag) if tag.type_expression.is_some() => {
                Some(*tag)
            }
            _ => None,
        })
    }

    /// `checkSatisfiesExpression` for each reparsed `@satisfies` cast `host`'s
    /// comment makes: the expression's type must satisfy the tag's, reported
    /// at the tag name (`getErrorSpanForNode`'s satisfies arm,
    /// `findOriginatingJSDocSatisfiesTag`, `scanner.go:2625`).
    pub(crate) fn check_jsdoc_satisfies_tags(&mut self, host: NodeId, ambient: bool) {
        let Some(doc) = self.jsdoc_entries.get(&host).and_then(|docs| docs.last()) else {
            return;
        };
        let Some(tsr_ast::JSDocTag::JSDocSatisfiesTag(tag)) = doc
            .tags
            .iter()
            .find(|tag| matches!(tag, tsr_ast::JSDocTag::JSDocSatisfiesTag(t) if t.type_expression.is_some()))
        else {
            return;
        };
        if ambient || self.file_has_parse_errors || !self.in_js_file(host) {
            return;
        }
        let Some(expression) = self.jsdoc_satisfies_target(host) else { return };
        let Some(annotation) = tag.type_expression.and_then(jsdoc_type_expression_type) else {
            return;
        };
        let (Some(at), Some(span_node)) = (tag.node_id, tag.tag_name.node_id) else { return };
        let span = self.nodes.span(span_node);
        self.check_satisfies_worker(at, expression, annotation, span);
    }

    /// The expression `reparseHosted`'s satisfies arm wraps for `host`.
    pub(crate) fn jsdoc_satisfies_target(&self, host: NodeId) -> Option<tsr_ast::Expression<'a>> {
        match self.node_map.get(host)? {
            Node::ParenthesizedExpression(node) => node.expression,
            Node::ReturnStatement(node) => node.expression,
            Node::ExportAssignment(node) => node.expression,
            Node::PropertyAssignment(node) => node.initializer,
            Node::PropertyDeclaration(node) => node.initializer,
            Node::VariableDeclaration(node) => node.initializer,
            Node::ShorthandPropertyAssignment(node) => node.object_assignment_initializer,
            Node::VariableStatement(node) => node
                .declaration_list?
                .declarations
                .iter()
                .find_map(|declaration| declaration.initializer),
            Node::ExpressionStatement(node) => match node.expression? {
                tsr_ast::Expression::BinaryExpression(binary)
                    if self.is_assignment_declaration(binary) =>
                {
                    binary.right
                }
                _ => None,
            },
            _ => None,
        }
    }
}

/// The type inside a tag's `{…}`.
fn jsdoc_type_expression_type(node: TypeNode<'_>) -> Option<TypeNode<'_>> {
    match node {
        TypeNode::JSDocTypeExpression(expression) => expression.r#type,
        other => Some(other),
    }
}
