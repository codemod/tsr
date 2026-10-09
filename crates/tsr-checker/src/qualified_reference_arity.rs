//! A qualified reference to a generic type written without its required type
//! arguments is `errorType`: `var b: A.B` where `A.B` is `class B<T>`.
//!
//! `getTypeReferenceType` (`checker.go:23146`) sends a class or interface to
//! `getTypeFromClassOrInterfaceReference` (`:23170`) and a type alias to
//! `getTypeFromTypeAliasReference` (`:23580`). Both count the written
//! arguments against `getMinTypeArgumentCount` (`:21938`, the index after the
//! last parameter without a default) and the parameter count. Outside the
//! permissive JS arm, a count outside that window reports TS2314/TS2707 and
//! answers `errorType`:
//!
//! - the class/interface arm keeps going in a JS file (`!isJs`, `:23195`), and
//!   reports nothing at all in JS without `noImplicitAny`;
//! - the alias arm answers `errorType` in every file.
//!
//! Its consumers read that `errorType` as `any`: `var b: A.B` prints `any`, and
//! `checkIdentifier`'s definite-assignment test (`checker.go:11156`) assumes
//! an `any`-typed variable initialized, so `b.foo()` reports no TS2454.
//!
//! TSR's unqualified road already answers the error. Its qualified road
//! (`qualified_type_reference`, `declared.rs`) minted a named object type for
//! an argument-less generic reference, so `b` looked like a real class
//! instance. This file is the arity test for that road's argument-less arm.
//! The TS2314 report itself is already made elsewhere (`type_argument_arity.rs`).
//!
//! No cache or side table: the type parameters are the declaration's own
//! (`local_type_parameters_of`).
//!
//! `docs/parity/notes/r6-smallcodes5.md` §2.6.

use tsr_ast::NodeId;
use tsr_binder::{SymbolFlags, SymbolId};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// Whether an argument-less reference at `site` to `resolved` is outside
    /// its arity window, so upstream answers `errorType` for it.
    pub(crate) fn argument_less_reference_is_error(
        &mut self,
        resolved: SymbolId,
        site: Option<NodeId>,
    ) -> bool {
        let flags = self.binder.symbols().get(resolved).flags;
        let is_alias = flags.contains(SymbolFlags::TYPE_ALIAS);
        if !is_alias && !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
            return false;
        }
        let parameters = self.local_type_parameters_of(resolved);
        // `getMinTypeArgumentCount`: one past the last parameter with no
        // default.
        let minimum =
            parameters.iter().rposition(|p| p.default_type.is_none()).map_or(0, |index| index + 1);
        if minimum == 0 {
            return false;
        }
        is_alias || !site.is_some_and(|site| self.in_js_file(site))
    }
}
