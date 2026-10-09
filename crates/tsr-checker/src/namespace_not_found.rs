//! TS2503 / TS2833 — `Cannot find namespace '{0}'.` and its spelling
//! suggestion, for an identifier resolved with `SymbolFlagsNamespace`.
//!
//! Two readers ask the question:
//!
//! - the left identifier of a qualified type name (`resolveEntityName`'s
//!   qualified arm, `checker.go:15782`), whose report TSR makes in
//!   `check_qualified_type_name_at` (`check.rs`);
//! - the identifier module reference of `import a = b`
//!   (`getSymbolOfPartOfRightHandSideOfImportEquals`, `checker.go:5020`): its
//!   "case 1", where an unqualified right side is resolved as a **namespace**,
//!   reached from `checkImportEqualsDeclaration` → `checkImportBinding` →
//!   `resolveAlias`.
//!
//! Both fail into `onFailedToResolveSymbol` (`checker.go:1564`) with
//! `meaning == SymbolFlagsNamespace` and `Cannot_find_namespace_0`. With that
//! meaning only three of its arms can answer: `checkAndReportErrorForUsingTypeAsNamespace`
//! (`:1608`), the suggested lib (`:1585`), and the spelling suggestion
//! (`:1591`). The other `checkAndReportErrorFor…` arms each need a value or
//! type meaning, an `extends` clause or an export specifier, none of which a
//! namespace lookup has.
//!
//! `docs/parity/notes/r6-smallcodes4.md` §2.1 records the hooks and their
//! measurement.

use tsr_ast::NodeId;
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

#[expect(
    dead_code,
    reason = "its two readers are hooked by docs/parity/notes/r6-smallcodes4-namespace-not-found.diff, which removes this"
)]
impl Checker<'_, '_> {
    /// `resolveName(location, name, SymbolFlagsNamespace)` as this port asks
    /// it: a module, an enum, or an alias. An alias is accepted whatever its
    /// target's meaning, as `check_qualified_type_name_at`'s lookup already
    /// does.
    ///
    /// **`globalThis` always answers.** Upstream puts a `Module` symbol named
    /// `globalThis` into `globals` (`checker.go:962-964`), so every
    /// namespace lookup of the name that reaches the globals finds it, and a
    /// local value of that name is not a namespace and does not shadow it.
    /// This port's binder has no such symbol (`binder.rs`,
    /// `merge_global_augmentation`); its readers spell the symbol as the
    /// name, as `typeof globalThis` already is (`expressions.rs`).
    pub(crate) fn resolves_as_namespace(&self, location: NodeId, name: &str) -> bool {
        name == "globalThis"
            || self
                .binder
                .resolve_name(
                    self.nodes,
                    self.node_map,
                    location,
                    name,
                    SymbolFlags::NAMESPACE | SymbolFlags::ALIAS,
                )
                .is_some()
    }

    /// `onFailedToResolveSymbol(location, name, SymbolFlagsNamespace,
    /// Cannot_find_namespace_0)` (`checker.go:1564`), for a name that
    /// [`Checker::resolves_as_namespace`] did not find.
    ///
    /// - **A type is a different error.** `checkAndReportErrorForUsingTypeAsNamespace`
    ///   (`:1608`) reports TS2702/TS2713 when the name resolves with
    ///   `SymbolFlagsType &^ SymbolFlagsNamespace`. That arm is not ported
    ///   (r5-smallcodes3 §3.2 holds it), so a type answers with no report, as
    ///   the port did before.
    /// - **A value is not.** No arm answers a value under a namespace meaning,
    ///   so `let intrinsic: intrinsic.intrinsic` with a local `intrinsic`
    ///   variable is TS2503 (`intrinsicKeyword.ts(10,20)`).
    /// - A suggested lib keeps TS2503 (`Cannot_find_namespace_0` takes the
    ///   lib as an unused second argument); otherwise a spelling suggestion
    ///   is TS2833.
    pub(crate) fn report_cannot_find_namespace(&mut self, location: NodeId, name: &str) {
        if self
            .binder
            .resolve_name(
                self.nodes,
                self.node_map,
                location,
                name,
                SymbolFlags::TYPE.difference(SymbolFlags::NAMESPACE),
            )
            .is_some()
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(location) else { return };
        let span = self.nodes.span(location);
        let suggestion = if crate::check::suggested_lib_for(name).is_some() {
            None
        } else {
            self.spelling_suggestion_for(location, name, SymbolFlags::NAMESPACE)
        };
        let diagnostic = match suggestion {
            Some(suggestion) => Diagnostic::with_args(
                &messages::CANNOT_FIND_NAMESPACE_0_DID_YOU_MEAN_1,
                span,
                [name.to_string(), suggestion],
            ),
            None => {
                Diagnostic::with_args(&messages::CANNOT_FIND_NAMESPACE_0, span, [name.to_string()])
            }
        };
        self.report(file, diagnostic);
    }

    /// `import a = b`: `getSymbolOfPartOfRightHandSideOfImportEquals`'s case 1
    /// (`checker.go:5020`), an identifier right side resolved as a namespace.
    pub(crate) fn check_import_equals_identifier_reference(&mut self, reference: NodeId) {
        let Some(name) = self.identifier_text(reference).map(str::to_string) else { return };
        // `resolveEntityName` returns nil for a missing name
        // (`ast.NodeIsMissing`, `checker.go:15773`): parser recovery's empty
        // identifier, as in `import abstract class D {}`.
        let span = self.nodes.span(reference);
        if name.is_empty() || span.start == span.end {
            return;
        }
        if self.resolves_as_namespace(reference, &name) {
            return;
        }
        self.report_cannot_find_namespace(reference, &name);
    }
}
