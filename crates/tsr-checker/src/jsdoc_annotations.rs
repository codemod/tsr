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

use tsr_ast::{Node, NodeId};

use crate::checker::Checker;

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
        let (annotation, initializer) = match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) if declaration.r#type.is_none() => {
                (self.jsdoc_type_annotation(node), declaration.initializer)
            }
            Some(Node::PropertyDeclaration(property)) if property.r#type.is_none() => {
                (self.jsdoc_cast_annotation(node), property.initializer)
            }
            _ => return,
        };
        let (Some(annotation), Some(initializer)) = (annotation, initializer) else { return };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(initializer);
        let Some(initializer_id) = initializer.node_id() else { return };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, initializer_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(node, initializer_id, source, target);
    }
}
