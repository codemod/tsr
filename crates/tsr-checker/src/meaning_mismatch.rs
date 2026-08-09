//! TS2661 / TS2693 / TS2708 / TS2709 / TS2749 — a name that resolves, but not
//! under the meaning the position wanted.
//!
//! `onFailedToResolveSymbol`'s cascade (`checker.go:1564`): four of the seven
//! `checkAndReportErrorForXxx` arms that run **before** the missing-lib /
//! spelling-suggestion / `Cannot find name` sequence
//! [`Checker::check_value_identifier`] already ports.
//!
//! # This is a debt with an address, not a discovery
//!
//! Both TS2304 rules already located the gap and left a comment on it —
//! `check_value_identifier`'s *"a name that resolves under another meaning gets
//! a different code, so silence is the only sound answer until those arms are
//! ported"*, and `check_type_reference_name`'s *"a name that resolves as a
//! value is TS2749, and as a namespace TS2709"*. Each rule's meaning ladder is
//! a loop that returns on the first hit; this module is what that hit means.
//!
//! `docs/architecture/checker-notes-diag2.md` §163.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `isPrimitiveTypeName` (`checker.go:1637`).
///
/// Six names, and **not** `undefined`, `null`, `void`, `object` or `symbol` —
/// upstream's list exactly.
fn is_primitive_type_name(name: &str) -> bool {
    matches!(name, "any" | "string" | "number" | "boolean" | "never" | "unknown")
}

/// `isES2015OrLaterConstructorName` (`checker.go:1703`).
fn is_es2015_or_later_constructor_name(name: &str) -> bool {
    matches!(name, "Promise" | "Symbol" | "Map" | "WeakMap" | "Set" | "WeakSet")
}

impl Checker<'_, '_> {
    /// The cascade for a name in a **type** position, in upstream's order.
    ///
    /// Returns `true` when it reported, which is the `||` chain's contract at
    /// `checker.go:1570`: the caller stops rather than falling through.
    ///
    /// **Only the two type-position arms are here.** §163 built all four and
    /// §164 measured the value-position arm at 238 wrong lines; these two
    /// measured zero wrong and zero converts, the signature of a correct rule
    /// that never fires. §166 found why — `resolve_name`'s globals fallback
    /// ignored `meaning`, so the ladder above always hit `TYPE` first — and
    /// this is that build's payoff.
    pub(crate) fn report_meaning_mismatch_in_type_position(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> bool {
        // `checkAndReportErrorForUsingNamespaceAsTypeOrValue` (`checker.go:1641`),
        // type branch: the position wanted `Type &^ Value`, and the name is a
        // module.
        if self.resolve_under(node, text, SymbolFlags::MODULE).is_some() {
            self.report_at(node, &messages::CANNOT_USE_NAMESPACE_0_AS_A_TYPE, text);
            return true;
        }
        // `checkAndReportErrorForUsingValueAsType` (`checker.go:1722`):
        // `resolveName(…, ^SymbolFlagsType & SymbolFlagsValue)`, and the symbol
        // must **not** also be a namespace — a namespace-and-value is the
        // `Cannot use namespace as a type` case, already handled above.
        let Some(symbol) = self.resolve_under(node, text, SymbolFlags::VALUE - SymbolFlags::TYPE)
        else {
            return false;
        };
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::NAMESPACE) {
            return false;
        }
        self.report_at(
            node,
            &messages::_0_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_0,
            text,
        );
        true
    }

    fn resolve_under(
        &self,
        node: NodeId,
        text: &str,
        meaning: SymbolFlags,
    ) -> Option<tsr_binder::SymbolId> {
        self.binder.resolve_name(self.nodes, self.node_map, node, text, meaning)
    }

    /// `c.error(errorLocation, message, name)` — both arms report at the same
    /// place with the same single argument.
    fn report_at(&mut self, node: NodeId, message: &'static tsr_diagnostics::Message, text: &str) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// The cascade for a name in a **value** position, in upstream's order.
    ///
    /// §164 measured this arm at **238 wrong lines** — against the pre-§166
    /// resolver, whose `globals` fallback ignored `meaning` and made every
    /// ladder above it return early. That number is not evidence about this
    /// code any more, which is why §169 re-measures rather than quotes it.
    pub(crate) fn report_meaning_mismatch_in_value_position(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> bool {
        // **A heritage clause is not this cascade's position in this port**,
        // and the bound goes FIRST rather than before the type-as-value arm.
        // Two reasons, both measured:
        //
        // - `is_value_reference` admits the names of `interface I extends A, B`
        //   and of `class C implements I` so that TS2304 still fires there — a
        //   position §165 verified this port is *right* to visit, but not to
        //   read a `TYPE` hit in as a meaning mismatch. §164 measured that at
        //   232 of 238 wrong lines.
        // - `class C1 extends M.I1` is upstream's **TS2689**,
        //   `checkAndReportErrorForExtendingInterface` — the cascade's *second*
        //   arm, which is not ported. It runs ahead of both namespace arms, so
        //   reporting TS2708 there is a wrong code at a right position.
        //   `classExtendsInterfaceInModule` is three of §169's wrong lines and
        //   every one of them is that. **Owner: the TS2689 arm.**
        if self.nodes.ancestors(node).any(|a| self.nodes.kind(a) == SyntaxKind::HeritageClause) {
            return false;
        }
        if self.report_exporting_primitive_type(node, text) {
            return true;
        }
        // `checkAndReportErrorForUsingNamespaceAsTypeOrValue`
        // (`checker.go:1641`), value branch.
        if self.resolve_under(node, text, SymbolFlags::NAMESPACE_MODULE).is_some() {
            // `export = ns` may legitimately name a namespace, and
            // `checkExportAssignment` decides whether that is an error.
            if !self.is_export_assignment_expression_name(node) {
                self.report_at(node, &messages::CANNOT_USE_NAMESPACE_0_AS_A_VALUE, text);
            }
            return true;
        }
        // `checkAndReportErrorForUsingTypeAsValue` (`checker.go:1662`).
        if is_primitive_type_name(text) {
            self.report_primitive_type_as_value(node, text);
            return true;
        }
        // `maybeMappedType`'s **syntactic half** (`checker.go:1710-1716`).
        // Upstream then asks a type question this port cannot ask here and
        // picks a different message when the answer is yes; declining the whole
        // shape suppresses rather than mis-codes. Owner: `checker_types`.
        if self.maybe_mapped_type_position(node) {
            return false;
        }
        // `resolveName(errorLocation, name, SymbolFlagsType &^ SymbolFlagsValue)`
        // — narrower than this port's ladder, and written upstream's way.
        let Some(symbol) = self.resolve_under(node, text, SymbolFlags::TYPE - SymbolFlags::VALUE)
        else {
            return false;
        };
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::VALUE) {
            return false;
        }
        if self.is_export_assignment_expression_name(node) {
            return true;
        }
        let message = if is_es2015_or_later_constructor_name(text) {
            &messages::_0_ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_ES2015_OR_LATER
        } else {
            &messages::_0_ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        };
        self.report_at(node, message, text);
        true
    }

    /// `checkAndReportErrorForExportingPrimitiveType` (`checker.go:1629`).
    fn report_exporting_primitive_type(&mut self, node: NodeId, text: &str) -> bool {
        if !is_primitive_type_name(text) {
            return false;
        }
        if self
            .nodes
            .parent(node)
            .is_none_or(|parent| self.nodes.kind(parent) != SyntaxKind::ExportSpecifier)
        {
            return false;
        }
        self.report_at(
            node,
            &messages::CANNOT_EXPORT_0_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE,
            text,
        );
        true
    }

    /// The primitive-name branch of `checkAndReportErrorForUsingTypeAsValue`
    /// (`checker.go:1664-1678`): three dedicated heritage messages, then
    /// TS2693.
    fn report_primitive_type_as_value(&mut self, node: NodeId, text: &str) {
        let grandparent = self.nodes.parent(node).and_then(|parent| self.nodes.parent(parent));
        let heritage = grandparent.filter(|g| self.nodes.kind(*g) == SyntaxKind::HeritageClause);
        if let Some(clause) = heritage
            && let Some(owner) = self.nodes.parent(clause)
        {
            let extends = matches!(
                self.node_map.get(clause),
                Some(Node::HeritageClause(c)) if c.token.kind == SyntaxKind::ExtendsKeyword
            );
            let owner_kind = self.nodes.kind(owner);
            let class_like =
                matches!(owner_kind, SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression);
            let message = if owner_kind == SyntaxKind::InterfaceDeclaration && extends {
                Some(&messages::AN_INTERFACE_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_0_IT_CAN_ONLY_EXTEND_OTHER_NAMED_OBJECT_TYPES)
            } else if class_like && extends {
                Some(&messages::A_CLASS_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_0_CLASSES_CAN_ONLY_EXTEND_CONSTRUCTABLE_VALUES)
            } else if class_like {
                Some(&messages::A_CLASS_CANNOT_IMPLEMENT_A_PRIMITIVE_TYPE_LIKE_0_IT_CAN_ONLY_IMPLEMENT_OTHER_NAMED_OBJECT_TYPES)
            } else {
                None
            };
            if let Some(message) = message {
                self.report_at(node, message, text);
            }
            // Upstream's `else` inside the heritage branch reports nothing and
            // still returns `true`. Faithful, and why this is not a fallthrough.
            return;
        }
        self.report_at(
            node,
            &messages::_0_ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            text,
        );
    }

    /// The syntactic half of `maybeMappedType` (`checker.go:1707`).
    fn maybe_mapped_type_position(&self, node: NodeId) -> bool {
        let mut at = node;
        loop {
            let Some(parent) = self.nodes.parent(at) else { return false };
            if !matches!(
                self.nodes.kind(parent),
                SyntaxKind::ComputedPropertyName | SyntaxKind::PropertySignature
            ) {
                return self.nodes.kind(parent) == SyntaxKind::TypeLiteral;
            }
            at = parent;
        }
    }

    /// `isExportAssignmentExpressionName` (`checker/utilities.go:144`).
    fn is_export_assignment_expression_name(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            if !matches!(
                self.nodes.kind(parent),
                SyntaxKind::PropertyAccessExpression | SyntaxKind::QualifiedName
            ) {
                break;
            }
            current = parent;
        }
        let Some(parent) = self.nodes.parent(current) else { return false };
        matches!(
            self.node_map.get(parent),
            Some(Node::ExportAssignment(assignment))
                if assignment.expression.and_then(|e| e.node_id()) == Some(current)
        )
    }
}
