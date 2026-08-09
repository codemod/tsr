//! TS2709 / TS2749 — a name in a TYPE position that resolves as a namespace or
//! as a value.
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

use tsr_ast::NodeId;
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

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
}
