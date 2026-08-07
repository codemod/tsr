//! The **check traversal** — the second road into the checker, the one that
//! reports.
//!
//! # Why this module exists, and why it is not a hook on the query road
//!
//! [ADR-0040](../../../docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
//! decisions (1) and (2), which are the two the ADR's own falsifiers left
//! standing. Upstream has two entry points into one engine:
//!
//! | entry | purpose | here |
//! |---|---|---|
//! | `checkSourceFile` → `checkSourceElement` (`checker.go:2196`, `:2241`) | reports diagnostics | **this module** |
//! | `getTypeOfNode` (`checker.go:31927`) | answers "what type is this node" | `types_producer::type_at_location` |
//!
//! Until this file existed the port had built the query road and none of the
//! traversal road, which is the whole reason `diagnostics` read **80/5,488
//! (1.46%)** while `checker_types` went 36% → 73.65%. A diagnostic is an eager
//! **side effect of a walk**, not a return value a consumer can ask for, so no
//! amount of work on the query road can produce one.
//!
//! # What it walks today, and why so little
//!
//! Only the declarations that carry a **module specifier**, plus the module
//! bodies that can contain them. That is deliberately far short of upstream's
//! `checkSourceElement`, and the reason is `docs/conventions.md`'s rule that a
//! diagnostic emitter is the consumer that *acts on the negative*: every node
//! kind this walk learns to visit is a new opportunity to report something
//! upstream does not, and under the suite's **exact multiset equality** an
//! invented diagnostic fails a case exactly as a missing one does — except that
//! it can also break a case that passes today.
//!
//! So the traversal grows one rule at a time, each sized against
//! `crates/tsr-conformance/examples/diaggap.rs` before it is written and scored
//! against a registered bar afterwards. See
//! `docs/architecture/checker-notes-diag2.md`.
//!
//! # Diagnostics carry the file they are in
//!
//! Upstream's `ast.DiagnosticsCollection` keys by file because a `Diagnostic`
//! there holds its `*ast.SourceFile`. [`tsr_diagnostics::Diagnostic`] holds a
//! [`tsr_core::Span`] and nothing else, and under
//! [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md) one
//! `NodeTable` spans every file of a program — so a bare span is ambiguous
//! across files and the collection stores `(source file, diagnostic)` pairs.
//! Getting this wrong would put every cross-file diagnostic on the wrong line of
//! the wrong unit, which is the failure mode that looks like a checker bug for a
//! week.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// Report every diagnostic this port can produce for one source file.
    ///
    /// `checkSourceFile` (`checker.go:2196`) → `checkSourceElements` →
    /// `checkSourceElement` (`checker.go:2241`). Upstream's version also runs
    /// grammar checks and deferred nodes; neither is ported.
    ///
    /// Idempotent by construction is *not* claimed: calling this twice on one
    /// file appends its diagnostics twice, exactly as upstream's would without
    /// its `checkSourceFileWorker` memo. Callers run it once per file.
    pub fn check_source_file(&mut self, file: NodeId) {
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return };
        for statement in source.statements {
            self.check_source_element(statement.node_id());
        }
    }

    /// One statement, and the statements a module body nests inside it.
    ///
    /// The recursion into `ModuleDeclaration` is not decoration: a
    /// `declare module "x" { import y = require("z"); }` puts an unresolvable
    /// specifier two levels down, and upstream reaches it through the general
    /// `checkSourceElement` walk. Nothing else recurses yet — a function body
    /// cannot contain an import declaration, and `import("x")` in expression or
    /// type position is a separate rule with its own sizing.
    fn check_source_element(&mut self, node: Option<NodeId>) {
        let Some(node) = node else { return };
        match self.node_map.get(node) {
            Some(Node::ImportDeclaration(declaration)) => {
                // `import "x"` with no clause is a **side-effect import**, and
                // upstream gives it its own message — `checkImportDeclaration`'s
                // `else if` branch at `checker.go:5321`, guarded by
                // `NoUncheckedSideEffectImports.IsTrueOrUnknown()`, which is
                // true when the option is unset. Same site, same resolution,
                // a different code.
                let side_effect = declaration.import_clause.is_none();
                if side_effect && !self.no_unchecked_side_effect_imports {
                    // `checker.go:5321`'s guard: with the option explicitly
                    // off, upstream does not resolve the specifier at all, so
                    // there is no diagnostic of any code to report here.
                    return;
                }
                self.check_module_specifier(
                    node,
                    declaration.module_specifier.and_then(|s| s.node_id()),
                    side_effect,
                );
            }
            Some(Node::ExportDeclaration(declaration)) => {
                self.check_module_specifier(
                    node,
                    declaration.module_specifier.and_then(|s| s.node_id()),
                    false,
                );
            }
            Some(Node::ImportEqualsDeclaration(declaration)) => {
                // Only the `require("x")` spelling names a module; `import a = b.c`
                // is an entity-name alias and resolves through the scope.
                if let Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) =
                    declaration.module_reference
                {
                    self.check_module_specifier(
                        node,
                        reference.expression.and_then(|e| e.node_id()),
                        false,
                    );
                }
            }
            Some(Node::ModuleDeclaration(declaration)) => match declaration.body {
                Some(tsr_ast::ModuleBody::ModuleBlock(block)) => {
                    for statement in block.statements {
                        self.check_source_element(statement.node_id());
                    }
                }
                Some(tsr_ast::ModuleBody::ModuleDeclaration(nested)) => {
                    self.check_source_element(nested.node_id);
                }
                None => {}
            },
            _ => {}
        }
    }

    /// `checkExternalImportOrExportDeclaration` (`checker.go:5332`) — the gate
    /// that runs **before** any resolution and can stop it.
    ///
    /// Only the arm that matters for a specifier that will not resolve is
    /// ported: an import or export declaration whose parent is neither a
    /// `SourceFile` nor a module block belonging to an **ambient** module is a
    /// grammar error (TS1147 `Import_declarations_in_a_namespace_cannot_reference_a_module`,
    /// or TS1148 for the export spelling) and upstream `return`s without
    /// resolving anything.
    ///
    /// This port emits neither 1147 nor 1148 — they are grammar checks with
    /// their own sizing — so the arm is a **refusal**: silence where upstream
    /// says something else. Reporting TS2307 inside a namespace was 14 of the 60
    /// wrong lines the first counterfactual measured, all in
    /// `compiler/privacyImportParseErrors` and its sibling.
    fn external_import_is_positioned_for_resolution(&self, declaration: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        if self.nodes.kind(parent) == SyntaxKind::SourceFile {
            return true;
        }
        // `inAmbientExternalModule` (`checker.go:5343`): a module *block* whose
        // own parent is an ambient module declaration.
        self.nodes.kind(parent) == SyntaxKind::ModuleBlock
            && self.nodes.parent(parent).is_some_and(|owner| self.is_ambient_module_node(owner))
    }

    /// `ast.IsAmbientModule` (`ast/utilities.go:1652`): a module declaration
    /// named by a **string literal**, or a `declare global` augmentation
    /// (`IsGlobalScopeAugmentation`, `:1690`).
    fn is_ambient_module_node(&self, node: NodeId) -> bool {
        let Some(Node::ModuleDeclaration(declaration)) = self.node_map.get(node) else {
            return false;
        };
        matches!(declaration.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
            || declaration.keyword.kind == SyntaxKind::GlobalKeyword
    }

    /// TS2307 — `Cannot find module '{0}' or its corresponding type declarations.`
    ///
    /// Upstream's site is the **fallthrough** of `resolveExternalModule`
    /// (`checker.go:15149`), reached from `resolveExternalModuleName`
    /// (`checker.go:15100`) after `getCannotResolveModuleNameErrorForSpecificModule`
    /// has declined to substitute a more specific message. The error node is the
    /// specifier literal itself, so the reported column is the **opening quote**
    /// — `badExternalModuleReference.errors.txt` records `(1,21)` for
    /// `import a1 = require("garbage")`, and 21 is the `"`.
    ///
    /// # This emits on a strict subset of upstream's condition, on purpose
    ///
    /// Upstream's function is 190 lines carrying **fourteen** distinct messages,
    /// and TS2307 is what is left when every one of them declines. Each of the
    /// guards below is a situation where upstream reports a *different code*, so
    /// emitting TS2307 there would be a false positive — and under the suite's
    /// exact-multiset rule a false positive costs a case exactly as a missing
    /// diagnostic does, while additionally being able to break a case that
    /// passes today. `docs/conventions.md`: the emitter is the consumer that
    /// acts on the negative, so its bound is part of the design and not an
    /// implementation shortcut.
    ///
    /// | declined here | upstream's code there |
    /// |---|---|
    /// | the specifier resolves to a file | TS2306 `File_0_is_not_a_module`, TS7016, the `node16` mode family — never 2307 |
    /// | a `declare module "x"` names it | resolved; no diagnostic |
    /// | a **pattern** ambient module could match (`declare module "foo/*"`) | resolved by `FindBestPatternMatch` (`checker.go:15364`), which this port does not implement — so a match is *possible* and silence is the only sound answer |
    /// | a Node core module name (`fs`, `path`, …) | TS2580/TS2591, substituted by `getCannotResolveModuleNameErrorForSpecificModule` (`checker.go:15109`) |
    /// | `@types/…` | TS6137 is emitted *as well*, so the multiset would still differ |
    ///
    /// # Where a `None` from resolution is *not* a missing module
    ///
    /// [`Checker::resolve_external_module_name`] answers `None` both for "no
    /// file resolved" and for "a file resolved but it is not a module". Those
    /// are two upstream codes, so this rule cannot be written against that
    /// function's `Option` — it asks the host directly. That distinction is the
    /// single most load-bearing line in this file.
    fn check_module_specifier(
        &mut self,
        declaration: NodeId,
        specifier: Option<NodeId>,
        side_effect: bool,
    ) {
        let Some(specifier) = specifier else { return };
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else { return };
        let text = literal.text;

        if !self.external_import_is_positioned_for_resolution(declaration) {
            return;
        }
        // `tryFindAmbientModule` (`checker.go:15154`), consulted before the host
        // exactly as `resolveExternalModule` does.
        if self.ambient_module_for_diagnostics(text).is_some() {
            return;
        }
        // A pattern ambient module is unported (`checker.go:15364`); declining
        // whenever one *exists* is the sound bound, not whenever one matches.
        if self.has_pattern_ambient_module() {
            return;
        }
        if is_node_core_module(text) {
            return;
        }
        if text.starts_with("@types/") {
            return;
        }
        let Some(importing) = self.source_file_of_for_diagnostics(specifier) else { return };
        // No host means no resolution was ever attempted, and "we did not look"
        // must not read as "it is not there".
        let Some(host) = self.module_host else { return };
        // `resolvedModule.IsResolved()` (`checker.go:15208`). A resolution that
        // named a file the program does not hold is upstream's TS7016 / TS6142 /
        // TS2306 territory — a *different code at the same position* — so it
        // must not read as "cannot find". Distinguishing that from "found
        // nothing" is why [`crate::resolution::ModuleHost`] grew a second
        // method; it was 15 of the 60 wrong lines the first counterfactual
        // measured.
        if host.module_resolution_found(importing, text) {
            return;
        }
        let span = self.nodes.span(specifier);
        let message = if side_effect {
            &messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF_0
        } else {
            &messages::CANNOT_FIND_MODULE_0_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        };
        self.report(importing, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// Append to the collection upstream keeps as `c.diagnostics`
    /// (`checker.go:661`), drained by `GetDiagnostics` (`checker.go:13951`).
    fn report(&mut self, file: NodeId, diagnostic: Diagnostic) {
        self.diagnostics.push((file, diagnostic));
    }

    /// Every diagnostic this checker has reported, with the file each is in.
    ///
    /// The consumer *drains*; it does not compute. That is ADR-0040 decision
    /// (1), and the reason the collection lives on the `Checker` rather than
    /// being assembled by whoever walks the tree.
    #[must_use]
    pub fn diagnostics(&self) -> &[(NodeId, Diagnostic)] {
        &self.diagnostics
    }

    /// Is there any `declare module "…*…"` in the program?
    ///
    /// Upstream keeps `c.patternAmbientModules`, filled while collecting
    /// globals; this port has no such list, so the question is asked of the
    /// binder's global table. **A name-shape test, and it is deliberately
    /// coarse**: a global whose name contains `*` can only have come from a
    /// pattern module declaration, and the cost of a false `true` is silence on
    /// one case rather than a wrong diagnostic on another.
    fn has_pattern_ambient_module(&self) -> bool {
        self.binder.globals().keys().any(|name| name.contains('*'))
    }

    /// [`Checker::ambient_module`] is private to `symbols`; this is the same
    /// question asked from here.
    ///
    /// Duplicated rather than exposed because the `symbols` version is on the
    /// resolution path and its `pub(crate)` surface is what
    /// `checker-notes-modobj.md` §10 pins; a second caller changing that
    /// function's visibility for a diagnostic is the kind of coupling that makes
    /// the next resolution change look risky.
    fn ambient_module_for_diagnostics(&self, name: &str) -> Option<tsr_binder::SymbolId> {
        if name == "." || name == ".." || name.starts_with("./") || name.starts_with("../") {
            return None;
        }
        let &symbol = self.binder.globals().get(name)?;
        let symbol = self.binder.merged_symbol(symbol);
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(tsr_binder::SymbolFlags::VALUE_MODULE)
            .then_some(symbol)
    }

    /// `ast.GetSourceFileOfNode`, reachable from this module.
    fn source_file_of_for_diagnostics(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        loop {
            if self.nodes.kind(current) == SyntaxKind::SourceFile {
                return Some(current);
            }
            current = self.nodes.parent(current)?;
        }
    }
}

/// `core.NodeCoreModules()` (`internal/core/nodemodules.go`).
///
/// Upstream substitutes a *different* message for these
/// (`getCannotResolveModuleNameErrorForSpecificModule`, `checker.go:15109`), so
/// the list is a **refusal list** here rather than a resolution one: a name on
/// it is never reported as TS2307. Transcribed from upstream rather than
/// guessed, because a name missing from it becomes a false positive and a name
/// wrongly on it costs only silence.
fn is_node_core_module(name: &str) -> bool {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    NODE_CORE_MODULES.contains(&bare) || name.starts_with("node:")
}

/// The names `core.NodeCoreModules()` returns.
const NODE_CORE_MODULES: &[&str] = &[
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];
