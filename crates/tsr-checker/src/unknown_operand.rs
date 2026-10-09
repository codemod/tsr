//! TS18046 / TS2571 — `'{0}' is of type 'unknown'.` / `Object is of type
//! 'unknown'.`
//!
//! The first arm of `checkNonNullTypeWithReporter` (`checker.go:7413`): under
//! `strictNullChecks` an operand whose type has the `Unknown` flag is reported
//! before any nullable fact is asked, and the operand's type becomes
//! `errorType`. The message names the operand when it is an entity name
//! expression whose text is shorter than 100 characters
//! (`entityNameToString`), and says `Object` otherwise.
//!
//! Every reader of `checkNonNullType` shares this arm, so this file holds the
//! arm alone and each reader calls it before its own nullable reporter:
//! the property and element access receiver (`checkNonNullExpression` in
//! `checkPropertyAccessExpression`, `checker.go:11249`, and
//! `checkElementAccessExpression`) and the operator operands
//! (`checkNonNullType` from `checkBinaryLikeExpressionWorker`,
//! `checkPrefixUnaryExpression` and `checkPostfixUnaryExpression`). The call
//! head already has its own copy (`check_non_null_callee`, `calls.rs`).
//!
//! `docs/parity/notes/r6-smallcodes4.md` §3.1 records the hooks and their
//! measurement.

use tsr_ast::NodeId;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

#[expect(
    dead_code,
    reason = "its readers are hooked by docs/parity/notes/r6-smallcodes4-unknown-operand.diff, which removes this"
)]
impl Checker<'_, '_> {
    /// `checkNonNullTypeWithReporter`'s `Unknown` arm (`checker.go:7414-7424`).
    ///
    /// Reports and answers `true` when `ty` is `unknown` under
    /// `strictNullChecks`; the caller then answers `errorType` and runs no
    /// nullable reporter. Answers `false`, reporting nothing, otherwise.
    pub(crate) fn report_unknown_operand(&mut self, operand: NodeId, ty: TypeId) -> bool {
        if !self.strict_null_checks || !self.store.get(ty).flags.intersects(TypeFlags::UNKNOWN) {
            return false;
        }
        let text = self.entity_name_expression_text(operand).filter(|text| text.len() < 100);
        let span = self.error_span(operand);
        let diagnostic = match text {
            Some(text) => Diagnostic::with_args(&messages::_0_IS_OF_TYPE_UNKNOWN, span, [text]),
            None => Diagnostic::new(&messages::OBJECT_IS_OF_TYPE_UNKNOWN, span),
        };
        if let Some(file) = self.source_file_of_for_diagnostics(operand) {
            self.report(file, diagnostic);
        }
        true
    }
}
