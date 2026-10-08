//! The checker-free half of declaration emit: `isolatedDeclarations` analysis.
//!
//! # What this is, and what it is not a port of
//!
//! This crate has **no typescript-go counterpart**, deliberately. Upstream's
//! declaration transform (`internal/transformers/declarations`) answers "does this
//! declaration need inference?" by *running the inference and watching it fail*,
//! through a `printer.EmitResolver` whose sole implementation sits on the 60k-line
//! checker. Fifteen of the twenty `TS9xxx` diagnostics are raised from
//! `tracker.go:92`, inside the `SymbolTracker` the checker's node builder calls
//! when it cannot build a type. There is no syntactic predicate upstream to port.
//!
//! So the rules here are written against TypeScript's `isolatedDeclarations`
//! *specification* — the `TS9xxx` diagnostic family — rather than ported. That is a
//! deliberate departure from the project's "port, don't reinvent" default; see
//! [ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md) for
//! the evidence and the falsifiers, and
//! `docs/architecture/isolated-declarations.md` for how the analysis is shaped.
//!
//! **Anchoring rule for this crate** (ADR-0021 §"What upstream-anchoring means"):
//! each rule names the `TS9xxx` code it implements and the upstream message
//! constant in `internal/diagnostics/`. Diagnostic codes are a stable public
//! contract; `transform.go`'s internal structure is not. Do **not** anchor items
//! here to `transformers/declarations` functions — the correspondence does not
//! exist, and asserting it would make the drift tracker file issues against a
//! fiction.
//!
//! **The exception is [`accessibility`]**, which *is* a port: upstream raises
//! the accessibility errors for names written in declarations from a syntactic
//! walk (`checkEntityNameVisibility`), so that module anchors to
//! `transformers/declarations` and asks the checker through a resolver trait.
//! See `docs/parity/notes/r4-declemit.md`.
//!
//! # Shape
//!
//! Two passes, both syntactic:
//!
//! 1. [`visibility`] — which declarations reach the `.d.ts` at all. A declaration
//!    that is not emitted cannot produce a declaration-emit error, and the corpus
//!    is emphatic about this: `isolatedDeclarationErrorsReturnTypes` contains three
//!    structurally identical classes, and only the two reachable from an `export`
//!    are diagnosed.
//! 2. [`rules`] — walk the visible declarations and report where a type would have
//!    had to be inferred.

pub mod accessibility;
pub mod rules;
pub mod visibility;

use tsr_ast::{NodeTable, SourceFile};
use tsr_diagnostics::Diagnostic;

/// Report the `isolatedDeclarations` errors for one file.
///
/// Diagnostics come back in source order, which is the order upstream's
/// `.errors.txt` baselines list them in.
#[must_use]
pub fn analyze<'a>(file: &'a SourceFile<'a>, nodes: &NodeTable) -> Vec<Diagnostic> {
    analyze_with_options(file, nodes, AnalysisOptions::default())
}

/// The compiler options the analysis reads.
#[derive(Debug, Clone, Copy)]
pub struct AnalysisOptions {
    /// `strictNullChecks`, on by default as in typescript-go.
    pub strict_null_checks: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self { strict_null_checks: true }
    }
}

/// [`analyze`] with explicit compiler options.
#[must_use]
pub fn analyze_with_options<'a>(
    file: &'a SourceFile<'a>,
    nodes: &NodeTable,
    options: AnalysisOptions,
) -> Vec<Diagnostic> {
    let javascript = file
        .node_id
        .is_some_and(|id| nodes.flags(id).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE));
    let commonjs = visibility::is_commonjs_module(file, javascript);
    let visible = visibility::visible_declarations(file, commonjs);
    let mut diagnostics = rules::check(file, nodes, &visible, commonjs, options.strict_null_checks);
    diagnostics.sort_by_key(|d| (d.span.start, d.message.code()));
    diagnostics
}
