//! Which identifiers native `checkIdentifier` reaches, for the slots
//! `Checker::is_value_reference` (`check.rs`) does not list.
//!
//! TS2304 here is reported from a walk over every identifier, gated by a
//! parent-slot allow-list. Native reports it from `getResolvedSymbol`
//! (`checker.go:13890`), so a name is diagnosed exactly when some check calls
//! `checkExpression` on it. This module holds the r6-names corrections to that
//! gate, each anchored to the native call that does or does not reach the
//! identifier. `docs/parity/notes/r6-names.md` records the measurements and the
//! apply order of the hooks in `check.rs`.
//!
//! No cache, side table or traversal state: every question is a bounded
//! parent walk over the immutable node table.

use tsr_ast::{ForInitializer, JsxTagNameExpression, Node, NodeId, SyntaxKind};

use crate::checker::Checker;
use crate::jsx_intrinsic::is_intrinsic_jsx_name;

/// Value slots native checks that `is_value_reference` has no arm for.
///
/// - `checkWithStatement` (`checker.go:4162`) calls `checkExpression` on the
///   `with` expression; only the *statement* is parsed under
///   `NodeFlagsInWithStatement` (`parser.go:1386`, `parseWithStatement`).
/// - A JSX opening, self-closing or closing tag that is not
///   `isJsxIntrinsicTagName` (`checker/utilities.go:1116`) is
///   `checkExpression`ed: `resolveJsxOpeningLikeElement` (`jsx.go:562`) for
///   the opening tag and `checkJsxElementDeferred` (`jsx.go:84`) for the
///   closing one. A property-access tag already reaches the
///   `PropertyAccessExpression` arm through its receiver.
#[allow(dead_code, reason = "hook: docs/parity/notes/r6-names-value-slots.diff")]
pub(crate) fn value_reference_slot(node: NodeId, parent: Node<'_>) -> bool {
    let value_tag = |tag: Option<JsxTagNameExpression<'_>>| {
        matches!(tag, Some(JsxTagNameExpression::Identifier(name))
            if name.node_id == Some(node) && !is_intrinsic_jsx_name(name.text))
    };
    match parent {
        Node::WithStatement(with) => with.expression.and_then(|e| e.node_id()) == Some(node),
        Node::JsxOpeningElement(element) => value_tag(element.tag_name),
        Node::JsxSelfClosingElement(element) => value_tag(element.tag_name),
        Node::JsxClosingElement(element) => value_tag(element.tag_name),
        _ => false,
    }
}

impl Checker<'_, '_> {
    /// Is `node` inside the expression of a `for…of` whose declaration list is
    /// empty?
    ///
    /// `checkForOfStatement` (`checker.go:4051`) checks a declaration-list
    /// initializer **only** through `checkVariableDeclarationList`; the
    /// right-hand side is reached from each declaration's type
    /// (`getTypeForVariableLikeDeclaration`, `checker.go:16664`,
    /// `checkRightHandSideOfForOf`). `for (var of X)` is TS1123 with no
    /// declaration, so nothing ever checks `X` or anything inside it.
    /// `checkForInStatement` (`checker.go:3988`) checks its expression
    /// unconditionally and is not affected.
    #[allow(dead_code, reason = "hook: docs/parity/notes/r6-names-empty-for-of.diff")]
    pub(crate) fn in_unchecked_for_of_expression(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            if let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(parent)
                && statement.kind.kind == SyntaxKind::ForOfStatement
                && statement.expression.and_then(|e| e.node_id()) == Some(current)
            {
                return matches!(statement.initializer,
                    Some(ForInitializer::VariableDeclarationList(list)) if list.declarations.is_empty());
            }
            current = parent;
        }
        false
    }
}
