//! TS2451 / TS2300 / TS2567 across files — the declaration merges the excludes
//! masks forbid.
//!
//! `reportMergeSymbolError` (`checker.go:14201`), reached from `mergeSymbol`'s
//! final `else` (`checker.go:14199`) when `target.Flags &
//! getExcludedSymbolFlags(source.Flags) != 0`.
//!
//! # Why the report is here and the detection is in the binder
//!
//! Upstream merges in the checker, so it can report inline. This port merges in
//! [`tsr_binder::BindResult`] — `Binder::merge_globals` unions each script
//! file's top-level locals into the program's globals — and a diagnostic pushed
//! there would be lost: `diagnostics_suite::reported_for` runs the binder once
//! per unit *on that unit alone*, so it never sees a cross-file collision, and
//! nothing collects the binder diagnostics of the cross-file bind inside
//! `program_for_case`.
//!
//! So the binder records `(target, source)` pairs and this reports them. The
//! split is not a workaround for the harness: it restores upstream's own
//! layering, and it is what gets the **file** right. Both declarations of a
//! cross-file redeclaration are, by construction, in different files, and only
//! a checker diagnostic carries a file node id for the suite to map
//! (`diagnostics_suite.rs:304`).
//!
//! `docs/architecture/checker-notes-diag2.md` §159.

use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// Every conflict the binder recorded, reported once for the program.
    ///
    /// Idempotent by the guard rather than by the caller: `check_source_file`
    /// is per file and these diagnostics are per program, so the first file
    /// checked pays for all of them. Which file that is does not matter —
    /// every diagnostic carries the file of the declaration it is placed on,
    /// not the file being walked.
    pub(crate) fn report_merge_conflicts(&mut self) {
        if self.merge_conflicts_reported {
            return;
        }
        self.merge_conflicts_reported = true;
        for &(target, source) in self.binder.merge_conflicts() {
            self.report_one_merge_conflict(target, source);
        }
        for &(target, source) in self.binder.alias_merges() {
            self.report_alias_merge_conflict(target, source);
        }
    }

    /// Whether `declaration` is in a plain JavaScript file
    /// (`ast.IsPlainJSFile`, `ast/utilities.go:2889`).
    fn is_plain_js_declaration(&self, declaration: tsr_ast::NodeId) -> bool {
        if !self.in_js_file(declaration) {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(declaration) else { return true };
        self.module_host.and_then(|host| host.is_plain_js_file(file)).unwrap_or(true)
    }

    /// `mergeSymbol`'s alias arm (`checker.go:14152-14164`), for a pair the
    /// binder declined because the target is an alias.
    ///
    /// Upstream resolves the (never transient, here) target with
    /// `resolveSymbol`; an unresolvable alias (`unknownSymbol`) takes the
    /// source silently, a resolved symbol the source's excludes miss is merged
    /// into (still declined here, `bd tsr-y4u.12`), and one they hit is
    /// `reportMergeSymbolError(target, source)` — on the **alias's**
    /// declarations and the source's, not the resolved symbol's. A chain this
    /// port cannot follow to a non-alias declines.
    ///
    /// `checkMergedGlobalUMDSymbol`: `export as namespace THREE` (an alias of
    /// the module) and `declare global { const THREE }` — the module is a
    /// `ValueModule`, which a `const` excludes. `misc-checks.md` §19.
    fn report_alias_merge_conflict(&mut self, target: SymbolId, source: SymbolId) {
        let resolved = self.resolve_alias_fully(target);
        let resolved = self.binder.merged_symbol(resolved);
        let resolved_flags = self.binder.symbols().get(resolved).flags;
        if resolved_flags.intersects(SymbolFlags::ALIAS) {
            return;
        }
        let source_flags = self.binder.symbols().get(source).flags;
        if (source_flags | resolved_flags).intersects(SymbolFlags::ASSIGNMENT)
            || !source_flags.excludes().intersects(resolved_flags)
        {
            return;
        }
        self.report_merge_symbol_error(target, source);
    }

    /// `reportMergeSymbolError` for one pair, and the arm that precedes it.
    fn report_one_merge_conflict(&mut self, target: SymbolId, source: SymbolId) {
        // **The synthesised `undefined` global is not a merge target
        // upstream.** `addUndefinedToGlobalsOrErrorOnRedeclaration`
        // (`checker.go:1452`) installs `c.undefinedSymbol` in `c.globals`
        // *only if nothing else declared the name*; when a file does, it
        // reports TS2397 on each non-type declaration and leaves the globals
        // entry alone — so `mergeSymbol` is never reached and neither is
        // `reportMergeSymbolError`.
        //
        // This port seeds the symbol in `Binder::declare_synthesised_globals`,
        // which runs per file bind, so the lib bind installs it and a later
        // file's `var undefined` merges into it. Declining here restores
        // upstream's silence at these positions. **The diagnostics upstream
        // does give — TS2397, plus TS2414 and TS2427 for the class and
        // interface spellings — are unported, and their owner is the checker's
        // collision checks, not this rule.** `undefinedTypeAssignment2`, `3`
        // and `4` are the five lines this costs, all of them wrong before the
        // decline.
        if self.binder.undefined_symbol() == Some(target) {
            return;
        }
        let target_flags = self.binder.symbols().get(target).flags;
        // `checker.go:14188` — the arm **between** the merge and
        // `reportMergeSymbolError`, and it is not a duplicate-identifier error
        // at all. Merging anything into a *non-instantiated* namespace is
        // TS2649, reported once, on the source's first declaration.
        //
        // Upstream exempts `c.globalThisSymbol` here so that `var globalThis`
        // does not draw two messages. This port has no `globalThis` symbol, so
        // the exemption has nothing to name; if one is ever synthesised, this
        // is the line that needs it.
        if target_flags.intersects(SymbolFlags::NAMESPACE_MODULE) {
            let name = self.binder.symbols().get(target).name.to_string();
            let Some(&first) = self.binder.symbols().get(source).declarations.first() else {
                return;
            };
            if let Some(file) = self.source_file_of_for_diagnostics(first) {
                let span = self.error_span(first);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::CANNOT_AUGMENT_MODULE_0_WITH_VALUE_EXPORTS_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENTITY,
                        span,
                        [name],
                    ),
                );
            }
            return;
        }
        self.report_merge_symbol_error(target, source);
    }

    /// `reportMergeSymbolError` (`checker.go:14201`).
    fn report_merge_symbol_error(&mut self, target: SymbolId, source: SymbolId) {
        let target_flags = self.binder.symbols().get(target).flags;
        let source_flags = self.binder.symbols().get(source).flags;
        // The three-way choice at `checker.go:14203-14212`. Both sides are
        // tested for each, which is the difference from the *same-file* site in
        // `Binder::declare_into_with_excludes`: there upstream tests only the
        // symbol already in the table (`binder.go:214`), because the incoming
        // declaration's flags are the `includes` argument and the conflict was
        // decided against `excludes`. Here the two symbols are peers.
        let either = target_flags | source_flags;
        let (message, needs_name): (&'static tsr_diagnostics::Message, bool) = if either
            .intersects(SymbolFlags::ENUM)
        {
            (
                    &messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                    false,
                )
        } else if either.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE) {
            (&messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE_0, true)
        } else {
            (&messages::DUPLICATE_IDENTIFIER_0, true)
        };
        // `symbolToString(source)` — the bare name at every position this
        // reaches, because a symbol that collides in the *global* table has no
        // container to qualify with.
        let name = self.binder.symbols().get(source).name.to_string();
        // `addDuplicateDeclarationErrorsForSymbols` (`checker.go:14225`) walks
        // both symbols' declarations and reports on each. The related-info
        // chain it also builds (`X_0_was_also_declared_here`, `X_and_here`) is
        // not ported — `Diagnostic` carries no related information, and the
        // `errors_baseline` parser treats `!!! related` lines as hints rather
        // than diagnostics (`errors_baseline.rs:22`), so the suite never asks.
        // `isSourcePlainJS` / `isTargetPlainJS` (`checker.go:14215-14218`):
        // upstream suppresses the report **per side**, for whichever of the
        // two symbols' first declaration is in a plain JavaScript file
        // (`ast.IsPlainJSFile`: no `@ts-check` directive and `checkJs`
        // unset). That is why `plainJSReservedStrict`'s `const eval` reports
        // nothing — its own side is skipped, and the other side is
        // `lib.d.ts`, which the suite does not walk. The program answers the
        // file's plainness; a host that cannot tell leaves every JavaScript
        // file plain, which can only suppress a report upstream would make.
        let mut declarations: Vec<tsr_ast::NodeId> = Vec::new();
        for side in [source, target] {
            let side_declarations = self.binder.symbols().get(side).declarations.to_vec();
            if side_declarations.first().is_some_and(|&first| self.is_plain_js_declaration(first)) {
                continue;
            }
            declarations.extend(side_declarations);
        }
        for declaration in declarations {
            let Some(file) = self.source_file_of_for_diagnostics(declaration) else { continue };
            // `getAdjustedNodeForError` then `NewDiagnosticForNode` — the
            // declaration's *name*, which is what `error_span` centralises
            // (§48). `class c1 {}` reports at the `c1`.
            //
            // `GetNameOfDeclaration` also names an `export as namespace N`,
            // which `error_span`'s declaration list does not; it is a
            // declaration only an alias merge reaches here (§19).
            let span = match self.node_map.get(declaration) {
                Some(tsr_ast::Node::NamespaceExportDeclaration(export)) => export
                    .name
                    .and_then(|name| name.node_id)
                    .map_or_else(|| self.error_span(declaration), |name| self.nodes.span(name)),
                // The reparsed `JSTypeAliasDeclaration`'s name.
                Some(
                    tsr_ast::Node::JSDocTypedefTag(tsr_ast::JSDocTypedefTag {
                        name: Some(tsr_ast::JSDocFullName::Identifier(name)),
                        ..
                    })
                    | tsr_ast::Node::JSDocCallbackTag(tsr_ast::JSDocCallbackTag {
                        name: Some(tsr_ast::JSDocFullName::Identifier(name)),
                        ..
                    }),
                ) => name
                    .node_id
                    .map_or_else(|| self.error_span(declaration), |id| self.nodes.span(id)),
                _ => self.error_span(declaration),
            };
            let diagnostic = if needs_name {
                Diagnostic::with_args(message, span, [name.clone()])
            } else {
                Diagnostic::new(message, span)
            };
            self.report(file, diagnostic);
        }
    }
}
