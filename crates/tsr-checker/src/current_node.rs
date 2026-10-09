//! `c.currentNode` (`checker.go:596`): the node whose check is running, where
//! the instantiation limits report TS2589.
//!
//! Upstream sets it, and resets `c.instantiationCount`, in three places:
//! `checkSourceElement` (`checker.go:2243`), `checkDeferredNode` (`:2507`)
//! and `checkExpressionEx` (`:7561`). Each saves the previous node and
//! restores it on the way out, so it is the innermost source element or
//! expression on the check stack. It is read by
//! `instantiateTypeWithAlias`'s depth/count guard (`checker.go:22118`),
//! `getConditionalType`'s tail-recursion guard (`:24311`) and a few other
//! reports that have no better node.
//!
//! This port's check is a document-order walk over every node
//! (`check.rs`'s `check_node`), not upstream's typed `checkSourceElement`
//! dispatch, so the hook there skips the kinds upstream never hands to
//! `checkSourceElement` and that `checkExpression` never sees as an
//! expression: names and tokens ([`is_current_node_kind`]). Expression
//! identifiers still become current, through `check_expression`'s own hook,
//! which is upstream's `checkExpressionEx`.
//!
//! Checker port convention (`docs/conventions.md`): one `Option<NodeId>` on
//! the checker, owned by the check stack. It is written only by the save and
//! restore pair below and read only by the reports. It has no cache, no side
//! table and no traversal. The reset of `instantiation_count` rides on the
//! same entry, as upstream's does. `docs/parity/notes/r5-spans.md` §2.
//!
//! Nothing reads the node yet. The TS2589 reporter and its call sites are
//! held (`docs/parity/notes/r5-spans-ts2589-report-sites.diff`): this port's
//! instantiation depth reaches 100 on programs where upstream's does not, so
//! reporting would add false TS2589s (r5-spans §2.3).

use tsr_ast::{NodeId, SyntaxKind};

use crate::checker::Checker;

/// The node kinds the walk makes current. Upstream's `checkSourceElement`
/// takes statements, declarations, members, parameters and type nodes, and
/// `checkExpressionEx` takes expressions. A name (`Identifier`,
/// `PrivateIdentifier`, `QualifiedName`, `ComputedPropertyName`) or a bare
/// token is neither, unless it is an expression, and then `check_expression`
/// makes it current when it is checked.
#[inline]
pub(crate) fn is_current_node_kind(kind: SyntaxKind) -> bool {
    !(kind.is_token()
        || matches!(
            kind,
            SyntaxKind::Identifier
                | SyntaxKind::PrivateIdentifier
                | SyntaxKind::QualifiedName
                | SyntaxKind::ComputedPropertyName
        ))
}

impl Checker<'_, '_> {
    /// The prologue of `checkSourceElement` / `checkDeferredNode` /
    /// `checkExpressionEx`: make `node` current and reset the per-node
    /// instantiation count. Returns the node to restore.
    #[inline]
    pub(crate) fn enter_current_node(&mut self, node: NodeId) -> Option<NodeId> {
        self.instantiation_count = 0;
        self.current_node.replace(node)
    }

    /// The epilogue: `c.currentNode = saveCurrentNode`.
    #[inline]
    pub(crate) fn leave_current_node(&mut self, saved: Option<NodeId>) {
        self.current_node = saved;
    }
}
