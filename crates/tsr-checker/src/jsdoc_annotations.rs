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

use crate::{checker::Checker, jsdoc_params::top_level_tags, types::TypeId};

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

    /// `getContextualType`'s arms for the two wrappers `reparseHosted` puts
    /// around an expression (`makeNewCast`, `parser/reparser.go:684`): a
    /// reparsed `@satisfies` answers its tag's type (`KindSatisfiesExpression`,
    /// `checker.go:29384`), and a reparsed `@type` cast its asserted type
    /// ([`Checker::jsdoc_cast_contextual_type`]).
    pub(crate) fn jsdoc_satisfies_contextual_type(&mut self, expression: NodeId) -> Option<TypeId> {
        match self.jsdoc_cast_contextual_type(expression) {
            JSDocCastContext::Cast(context) => return context,
            JSDocCastContext::Satisfies | JSDocCastContext::NotWrapped => {}
        }
        let tag = self.jsdoc_satisfies_tag_of(expression)?;
        let annotation = jsdoc_type_expression_type(tag.type_expression?)?;
        Some(self.get_type_from_type_node(annotation))
    }

    /// `getContextualType`'s `KindAsExpression` arm (`checker.go:29384`) for
    /// the cast `reparseHosted`'s `KindJSDocTypeTag` arm makes of a `return`'s
    /// or a parenthesized expression's operand (`parser/reparser.go:378`):
    /// the asserted type, or no context for `@type {const}`
    /// (`isConstTypeReference`).
    ///
    /// Each typed `@type` and `@satisfies` tag of the host's last comment
    /// wraps the operand again, in tag order, so the operand's parent is the
    /// wrapper of the **first** such tag; a `@satisfies` there is
    /// [`Checker::jsdoc_satisfies_tag_of`]'s. The typing road
    /// ([`Checker::jsdoc_cast_annotation`]) reads the first typed `@type` of
    /// any comment; they differ only where the walk in `jsdoc_checks.rs`
    /// already records the same difference.
    ///
    /// No cache: one parent probe and one `jsdoc_entries` probe, which a
    /// TypeScript file answers from lib comments only on a `return` or a
    /// parenthesized expression; `in_js_file` is asked after a comment is
    /// found.
    fn jsdoc_cast_contextual_type(&mut self, expression: NodeId) -> JSDocCastContext {
        let Some(parent) = self.nodes.parent(expression) else {
            return JSDocCastContext::NotWrapped;
        };
        let operand = match self.node_map.get(parent) {
            Some(Node::ParenthesizedExpression(node)) => node.expression,
            Some(Node::ReturnStatement(node)) => node.expression,
            _ => return JSDocCastContext::NotWrapped,
        };
        if operand.and_then(|operand| operand.node_id()) != Some(expression) {
            return JSDocCastContext::NotWrapped;
        }
        let Some(doc) = self.jsdoc_entries.get(&parent).and_then(|docs| docs.last()) else {
            return JSDocCastContext::NotWrapped;
        };
        let mut cast = None;
        for tag in top_level_tags(doc.tags) {
            match tag {
                tsr_ast::JSDocTag::JSDocTypeTag(tag) => {
                    if let Some(Node::JSDocTypeExpression(expression)) = tag.type_expression
                        && let Some(annotation) = expression.r#type
                    {
                        cast = Some(annotation);
                        break;
                    }
                }
                tsr_ast::JSDocTag::JSDocSatisfiesTag(tag) if tag.type_expression.is_some() => {
                    return JSDocCastContext::Satisfies;
                }
                _ => {}
            }
        }
        let Some(annotation) = cast else { return JSDocCastContext::NotWrapped };
        if !self.in_js_file(parent) {
            return JSDocCastContext::NotWrapped;
        }
        if crate::assertions::is_const_type_reference(annotation) {
            return JSDocCastContext::Cast(None);
        }
        JSDocCastContext::Cast(Some(self.get_type_from_type_node(annotation)))
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

    /// Whether, in a JS file, a reparsed JSDoc node gives `position` (an
    /// expression, the child of its parent being asked) a contextual type
    /// where the written tree has none: `getContextualType` (`checker.go:29343`)
    /// meets a `makeNewCast` wrapper (`reparseHosted`'s `@type` cast on a
    /// `return` or a parenthesized expression, `parser/reparser.go:378`, or
    /// any `@satisfies`, `:396`), or the parent's `Type()` is a reparsed
    /// `@type` (`:346`–`:376`): a variable, property or property assignment
    /// read by `getContextualTypeForInitializerExpression`, an export
    /// assignment, or an assignment declaration's binary.
    ///
    /// The written-tree proof of absence (`has_no_contextual_type`,
    /// `signatures.rs`) asks this first, so a JS position with one of these
    /// is never shown context-free. It never answers for a TypeScript node.
    ///
    /// No cache: the readers are `jsdoc_entries` probes that almost always
    /// miss; the caller asks `in_js_file` once per walk.
    #[expect(dead_code, reason = "called by docs/parity/notes/r6-jsdoc2-js-implicit-any.diff")]
    pub(crate) fn jsdoc_reparse_gives_context(&self, position: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(position) else { return false };
        if self.jsdoc_satisfies_tag_of(position).is_some() {
            return true;
        }
        let is = |e: Option<tsr_ast::Expression<'_>>| e.and_then(|e| e.node_id()) == Some(position);
        match self.node_map.get(parent) {
            Some(Node::ParenthesizedExpression(node)) if is(node.expression) => {
                self.jsdoc_first_cast_tag(parent)
            }
            Some(Node::ReturnStatement(node)) if is(node.expression) => {
                self.jsdoc_first_cast_tag(parent)
            }
            Some(Node::VariableDeclaration(node)) if is(node.initializer) => {
                self.jsdoc_type_annotation(parent).is_some()
            }
            Some(Node::PropertyDeclaration(node)) if is(node.initializer) => {
                self.jsdoc_self_hosted_type(parent).is_some()
            }
            Some(Node::PropertyAssignment(node)) if is(node.initializer) => {
                self.jsdoc_self_hosted_type(parent).is_some()
            }
            Some(Node::ExportAssignment(node)) if is(node.expression) => {
                self.jsdoc_export_assignment_type(parent).is_some()
            }
            Some(Node::BinaryExpression(binary)) if is(binary.right) => {
                self.jsdoc_binary_type(parent, binary).is_some()
            }
            _ => false,
        }
    }

    /// Whether `host`'s last comment has a typed `@type` tag, the one
    /// [`Checker::jsdoc_cast_contextual_type`] reads as a cast.
    #[expect(dead_code, reason = "called by docs/parity/notes/r6-jsdoc2-js-implicit-any.diff")]
    fn jsdoc_first_cast_tag(&self, host: NodeId) -> bool {
        self.jsdoc_entries.get(&host).and_then(|docs| docs.last()).is_some_and(|doc| {
            top_level_tags(doc.tags).into_iter().any(|tag| {
                matches!(tag, tsr_ast::JSDocTag::JSDocTypeTag(tag)
                    if matches!(tag.type_expression,
                        Some(Node::JSDocTypeExpression(expression)) if expression.r#type.is_some()))
            })
        }) && self.in_js_file(host)
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

/// What [`Checker::jsdoc_cast_contextual_type`] found as an operand's
/// innermost reparsed wrapper.
enum JSDocCastContext {
    /// No reparsed cast or `@satisfies` wraps the operand.
    NotWrapped,
    /// A `@satisfies` wraps it first.
    Satisfies,
    /// A `@type` cast wraps it first: its context (`None` for `const`).
    Cast(Option<TypeId>),
}

/// The type inside a tag's `{…}`.
fn jsdoc_type_expression_type(node: TypeNode<'_>) -> Option<TypeNode<'_>> {
    match node {
        TypeNode::JSDocTypeExpression(expression) => expression.r#type,
        other => Some(other),
    }
}
