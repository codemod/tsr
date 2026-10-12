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
        let mut pending = DuplicateDeclarationErrors::default();
        for &(target, source) in self.binder.merge_conflicts() {
            self.report_one_merge_conflict(target, source, &mut pending);
        }
        for &(target, source) in self.binder.alias_merges() {
            self.report_alias_merge_conflict(target, source, &mut pending);
        }
        for (file, diagnostic) in pending.diagnostics {
            self.report(file, diagnostic);
        }
        self.report_class_prototype_merge_conflicts();
    }

    /// `mergeSymbolTable(target.Exports, source.Exports)` for a class merged
    /// with a namespace across files: the namespace's exported `prototype`
    /// meets the class's synthetic `prototype` property
    /// (`bindClassLikeDeclaration`, `binder.go:962`), `mergeSymbol`'s excludes
    /// test fails, and `reportMergeSymbolError` (`checker.go:14201`) reports
    /// on the declarations of both sides — only the namespace's, since the
    /// synthetic symbol has none.
    ///
    /// This binder neither creates the synthetic symbol nor records the pair,
    /// so the global table is scanned for a class symbol whose exported
    /// `prototype` is declared in a file none of the class's declarations is
    /// in. The same-file collision is the binder's upstream (`binder.go:965`)
    /// and stays `check_merged_namespace_prototype`'s, at the class.
    ///
    /// Reported here because every checker runs the merge
    /// (`initializeChecker`), so the checker that owns the namespace's file
    /// has it under several checkers (`mergedClassWithNamespacePrototype`,
    /// `tsr-2zk.1258`). One pass over the global table per checker.
    fn report_class_prototype_merge_conflicts(&mut self) {
        let classes: Vec<SymbolId> = self
            .binder
            .globals()
            .values()
            .map(|&symbol| self.binder.merged_symbol(symbol))
            .filter(|&symbol| {
                self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::CLASS)
            })
            .collect();
        for class in classes {
            let entry = self.binder.symbols().get(class);
            let Some(&prototype) = entry.exports.get("prototype") else { continue };
            let class_files: Vec<tsr_ast::NodeId> = entry
                .declarations
                .iter()
                .filter(|&&declaration| {
                    matches!(
                        self.nodes.kind(declaration),
                        tsr_ast::SyntaxKind::ClassDeclaration
                            | tsr_ast::SyntaxKind::ClassExpression
                    )
                })
                .filter_map(|&declaration| self.source_file_of_for_diagnostics(declaration))
                .collect();
            let prototype_entry = self.binder.symbols().get(prototype);
            let source_flags = prototype_entry.flags;
            let declarations = prototype_entry.declarations.to_vec();
            let Some(&first) = declarations.first() else { continue };
            // A `static prototype` member shares this port's exports table;
            // upstream binds it into the class's own members (TS2699).
            if self.nodes.parent(first).is_some_and(|parent| {
                matches!(
                    self.nodes.kind(parent),
                    tsr_ast::SyntaxKind::ClassDeclaration | tsr_ast::SyntaxKind::ClassExpression
                )
            }) {
                continue;
            }
            if self
                .source_file_of_for_diagnostics(first)
                .is_some_and(|file| class_files.contains(&file))
            {
                continue;
            }
            // `isSourcePlainJS`; the synthetic target has no declarations.
            if self.is_plain_js_declaration(first) {
                continue;
            }
            // The target's flags are `Property | Prototype`, so only the
            // source can make the message an enum or block-scoped one.
            let message = if source_flags.intersects(SymbolFlags::ENUM) {
                None
            } else if source_flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE) {
                Some(&messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE_0)
            } else {
                Some(&messages::DUPLICATE_IDENTIFIER_0)
            };
            for declaration in declarations {
                let Some(file) = self.source_file_of_for_diagnostics(declaration) else { continue };
                let span = self.error_span(declaration);
                let diagnostic = match message {
                    Some(message) => {
                        Diagnostic::with_args(message, span, ["prototype".to_string()])
                    }
                    None => Diagnostic::new(
                        &messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                        span,
                    ),
                };
                self.report(file, diagnostic);
            }
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
    fn report_alias_merge_conflict(
        &mut self,
        target: SymbolId,
        source: SymbolId,
        pending: &mut DuplicateDeclarationErrors,
    ) {
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
        self.report_merge_symbol_error(target, source, pending);
    }

    /// `reportMergeSymbolError` for one pair, and the arm that precedes it.
    fn report_one_merge_conflict(
        &mut self,
        target: SymbolId,
        source: SymbolId,
        pending: &mut DuplicateDeclarationErrors,
    ) {
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
        self.report_merge_symbol_error(target, source, pending);
    }

    /// `reportMergeSymbolError` (`checker.go:14201`).
    fn report_merge_symbol_error(
        &mut self,
        target: SymbolId,
        source: SymbolId,
        pending: &mut DuplicateDeclarationErrors,
    ) {
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
        // `isSourcePlainJS` / `isTargetPlainJS` (`checker.go:14215-14218`):
        // upstream suppresses the report **per side**, for whichever of the
        // two symbols' first declaration is in a plain JavaScript file
        // (`ast.IsPlainJSFile`: no `@ts-check` directive and `checkJs`
        // unset). That is why `plainJSReservedStrict`'s `const eval` reports
        // nothing — its own side is skipped, and the other side is
        // `lib.d.ts`, which the suite does not walk. The program answers the
        // file's plainness; a host that cannot tell leaves every JavaScript
        // file plain, which can only suppress a report upstream would make.
        let source_declarations = self.binder.symbols().get(source).declarations.to_vec();
        let target_declarations = self.binder.symbols().get(target).declarations.to_vec();
        for (side, other) in [
            (&source_declarations, &target_declarations),
            (&target_declarations, &source_declarations),
        ] {
            if side.first().is_some_and(|&first| self.is_plain_js_declaration(first)) {
                continue;
            }
            // `addDuplicateDeclarationErrorsForSymbols` (`checker.go:14225`).
            for &declaration in side {
                self.add_duplicate_declaration_error(
                    declaration,
                    message,
                    needs_name.then_some(name.as_str()),
                    &name,
                    other,
                    pending,
                );
            }
        }
    }

    /// `getAdjustedNodeForError` then `NewDiagnosticForNode`: the
    /// declaration's *name*, which is what `error_span` centralises (§48).
    /// `class c1 {}` reports at the `c1`. `GetNameOfDeclaration` also names
    /// an `export as namespace N`, which `error_span`'s declaration list does
    /// not; it is a declaration only an alias merge reaches here (§19).
    fn duplicate_declaration_span(&self, declaration: tsr_ast::NodeId) -> tsr_core::Span {
        match self.node_map.get(declaration) {
            Some(tsr_ast::Node::NamespaceExportDeclaration(export)) => export
                .name
                .and_then(|name| name.node_id)
                .map_or_else(|| self.error_span(declaration), |name| self.nodes.span(name)),
            // `GetNameOfDeclaration`'s import and export arms: the local
            // name (`export import VNode = react.ReactNode` reports at
            // `VNode`; r7-grammar §57).
            Some(
                tsr_ast::Node::ImportEqualsDeclaration(tsr_ast::ImportEqualsDeclaration {
                    name: Some(name),
                    ..
                })
                | tsr_ast::Node::ImportClause(tsr_ast::ImportClause { name: Some(name), .. })
                | tsr_ast::Node::ImportSpecifier(tsr_ast::ImportSpecifier {
                    name: Some(name), ..
                }),
            ) => {
                name.node_id.map_or_else(|| self.error_span(declaration), |id| self.nodes.span(id))
            }
            Some(tsr_ast::Node::ExportSpecifier(tsr_ast::ExportSpecifier {
                name: Some(name),
                ..
            })) => tsr_ast::Node::from(*name)
                .node_id()
                .map_or_else(|| self.error_span(declaration), |id| self.nodes.span(id)),
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
            ) => {
                name.node_id.map_or_else(|| self.error_span(declaration), |id| self.nodes.span(id))
            }
            // `getAdjustedNodeForError`: the declaration's name when it has
            // one (a method signature's too, which `error_span` would not
            // narrow to), else the declaration.
            _ => self
                .declaration_name_of(declaration)
                .map_or_else(|| self.error_span(declaration), |name| self.error_span(name)),
        }
    }

    /// `addDuplicateDeclarationError` (`checker.go:14232`): the error at the
    /// declaration's adjusted node, found or issued by `lookupOrIssueError`
    /// (found only while an identical earlier error has no related
    /// information yet), then for each related
    /// declaration not at the error's own node, `'{0}' was also declared
    /// here.` first and `and here.` after, skipping one already present and
    /// stopping at five (r7-grammar §34).
    fn add_duplicate_declaration_error(
        &self,
        declaration: tsr_ast::NodeId,
        message: &'static tsr_diagnostics::Message,
        argument: Option<&str>,
        symbol_name: &str,
        related: &[tsr_ast::NodeId],
        pending: &mut DuplicateDeclarationErrors,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(declaration) else { return };
        let span = self.duplicate_declaration_span(declaration);
        let key = (file, span.start, span.end, message.code(), argument.map(str::to_string));
        // `lookupOrIssueError`: `DiagnosticsCollection.Lookup` compares with
        // `CompareDiagnostics`, related information included, so it finds an
        // earlier error only while that error is still bare. One that took
        // related nodes is a different diagnostic, and the new error is
        // issued beside it. `SortAndDeduplicateDiagnostics`'
        // `compactAndMergeRelatedInfos` (`compiler/program.go:1459`) folds the
        // two, sorting and deduplicating their related lists.
        let index = if let Some(&index) = pending.index.get(&key)
            && pending.diagnostics[index].1.related_information().is_empty()
        {
            index
        } else {
            let diagnostic = match argument {
                Some(argument) => Diagnostic::with_args(message, span, [argument.to_string()]),
                None => Diagnostic::new(message, span),
            };
            pending.diagnostics.push((file, diagnostic));
            pending.index.insert(key, pending.diagnostics.len() - 1);
            pending.diagnostics.len() - 1
        };
        for &other in related {
            let Some(other_file) = self.source_file_of_for_diagnostics(other) else { continue };
            let other_span = self.duplicate_declaration_span(other);
            if other_file == file && other_span == span {
                continue;
            }
            let existing = pending.diagnostics[index].1.related_information();
            let other_name = self.module_host.and_then(|host| host.file_path(other_file));
            let already = existing.iter().any(|each| {
                each.span == other_span
                    && each.file().map(|f| f.file_name().to_string()) == other_name
                    && matches!(each.message.code(), 6203 | 6204)
            });
            if existing.len() >= 5 || already {
                continue;
            }
            let related = if existing.is_empty() {
                self.related_diagnostic_at(
                    other,
                    &messages::_0_WAS_ALSO_DECLARED_HERE,
                    other_span,
                    vec![symbol_name.to_string()],
                )
            } else {
                self.related_diagnostic_at(other, &messages::AND_HERE, other_span, Vec::new())
            };
            pending.diagnostics[index].1.add_related_information(Some(related));
        }
    }
}

/// The merge-conflict pass's errors, in issue order, with the latest error
/// per `lookupOrIssueError` identity (file, span, code, argument) indexed.
/// Owned by one `report_merge_conflicts` call and reported at its end; no
/// state outlives it.
#[derive(Default)]
struct DuplicateDeclarationErrors {
    diagnostics: Vec<(tsr_ast::NodeId, Diagnostic)>,
    index: rustc_hash::FxHashMap<(tsr_ast::NodeId, u32, u32, u32, Option<String>), usize>,
}
