//! The arity window of a generic type reference, as the TYPE it answers.
//!
//! `getTypeFromClassOrInterfaceReference` (`checker.go:23169`) and
//! `getTypeFromTypeAliasReference` (`checker.go:23580`) both test the written
//! argument count against `[getMinTypeArgumentCount, len(typeParameters)]`
//! (`checker.go:21938`) and, outside it, report TS2314/TS2707 and answer
//! **`errorType`** — upstream's computed answer, not a failure to compute.
//! That `errorType` is an ordinary type that composes: `IFoo[]` is `any[]`
//! and `C<I>` is `C<any>` (`genericTypeReferencesRequireTypeArgs`).
//!
//! This port's [`crate::Checker::get_instantiated_type_reference`] answered
//! its gap marker (`intrinsics.error`, ADR-0038) for every count outside the
//! window it can fill, so an enclosing array or reference declined as well
//! (`>c2 : C<any>` recorded `any`). The query here separates the two: a count
//! outside native's window is native's `errorType`
//! ([`crate::intrinsics::Intrinsics::native_error`], ADR-0048); a count
//! inside it that the port cannot fill stays the gap. The diagnostics are
//! [`crate::type_argument_arity`]'s and are not touched.
//!
//! See `docs/parity/notes/r6-typesroots.md` §2.

use crate::checker::Checker;
use tsr_binder::{SymbolFlags, SymbolId};

impl<'a> Checker<'a, '_> {
    /// Whether `node`, a reference to the generic `symbol`, is written outside
    /// native's arity window, so the reference answers `errorType`.
    ///
    /// The JS leg of `getTypeFromClassOrInterfaceReference` never answers
    /// `errorType` (`if !isJs { return c.errorType }`; with `noImplicitAny`
    /// off it does not even report), so a class or interface referenced from
    /// a JavaScript file is `false` here and keeps the fill road. An alias
    /// reference has no JS exception (`checker.go:23596`).
    pub(crate) fn reference_arity_answers_error_type(
        &self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        symbol: SymbolId,
    ) -> bool {
        let flags = self.binder.symbols().get(symbol).flags;
        let class_or_interface = flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE);
        if !class_or_interface && !flags.contains(SymbolFlags::TYPE_ALIAS) {
            return false;
        }
        if class_or_interface
            && node.node_id.and_then(|id| self.source_file_of(id)).is_some_and(|file| {
                self.nodes.flags(file).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE)
            })
        {
            return false;
        }
        let parameters = self.local_type_parameters_of(symbol);
        if parameters.is_empty() {
            return false;
        }
        // getMinTypeArgumentCount: one past the last parameter WITHOUT a
        // default (`hasTypeParameterDefault`).
        let minimum = parameters
            .iter()
            .rposition(|declaration| declaration.default_type.is_none())
            .map_or(0, |index| index + 1);
        let written = node.type_arguments.len();
        written < minimum || written > parameters.len()
    }
}
