//! One file in a program: its source, its tree, and the range of the program's
//! node table it filled.
//!
//! # Why this owns nothing
//!
//! It used to own everything. Until the identity widening
//! ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md))
//! each file had its own arena, its own `NodeTable`, and its own `BindResult`,
//! bundled by `self_cell` so the self-referential whole could be moved and sent
//! between threads. That is what per-file identity *is*: a file was the unit an
//! id was meaningful beside.
//!
//! Under program-wide identity the arena, the node table, the node map and the
//! bind result all belong to the [`crate::Program`], and every one of them
//! outlives every file in it. So a file is a plain borrowing struct: nothing
//! self-referential is left to hold together, the `self_cell` is gone, and with
//! it the `unsafe impl Send` that was a named exception in
//! [ADR-0011](../../../docs/adr/0011-unsafe-is-opt-in.md).
//!
//! # What replaced `is_bound`
//!
//! Upstream binds per file behind `file.IsBound()`
//! (`internal/compiler/program.go`). One `SymbolStore` cannot be filled from
//! two places at once, so binding here is a single accumulation in program
//! order and "which files are bound" is a *prefix*, tracked by
//! [`crate::Program::bound_file_count`] rather than by a flag per file. The
//! phase separation upstream has is preserved; only its bookkeeping is
//! coarser.

use std::ops::Range;

use tsr_ast::{NodeId, SourceFile};
use tsr_diagnostics::Diagnostic;
use tsr_parser::{FileReferences, JSDocTable};
use tsr_path::Path;

/// A file in a program.
///
/// Every reference borrows from the arena the [`crate::Program`] was built in,
/// including the file's own name and text — the loader copies both in with
/// `Arena::alloc_str`, because a `Symbol`'s name borrows the source text and a
/// program-wide `SymbolStore` therefore outlives any per-file storage. See
/// ADR-0034, "Who owns the arena".
#[derive(Debug)]
pub struct ProgramFile<'a> {
    /// The file's identity, and the key it is stored under.
    path: Path,
    file_name: &'a str,
    text: &'a str,
    source_file: &'a SourceFile<'a>,
    jsdoc: JSDocTable<'a>,
    /// The half-open run of ids this file's parse claimed in the program's
    /// shared node table.
    ///
    /// Contiguous, because files are parsed one at a time — which is what makes
    /// "which file does this node belong to" answerable at all once a `NodeId`
    /// spans the program. A `Span` is still an offset into *this* file's text,
    /// so something has to answer that question, and this is what answers it.
    node_range: Range<u32>,
    diagnostics: Vec<Diagnostic>,
    file_references: FileReferences,
}

impl<'a> ProgramFile<'a> {
    /// Assemble a file from what the loader parsed into the program's tables.
    ///
    /// Crate-internal: a `ProgramFile` is only meaningful beside the tables its
    /// ids index, so there is no way to build one that is not the loader's.
    pub(crate) fn new(
        path: Path,
        file_name: &'a str,
        text: &'a str,
        parsed: tsr_parser::ParsedInto<'a>,
    ) -> Self {
        Self {
            path,
            file_name,
            text,
            source_file: parsed.source_file,
            jsdoc: parsed.jsdoc,
            node_range: parsed.node_range,
            diagnostics: parsed.diagnostics,
            file_references: parsed.file_references,
        }
    }

    /// The file's canonical identity.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file name as it was given, before canonicalisation.
    #[must_use]
    pub fn file_name(&self) -> &'a str {
        self.file_name
    }

    /// The source text.
    #[must_use]
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// The root of this file's tree.
    ///
    /// A plain getter rather than the `with_ast` closure this had under
    /// `self_cell`: the tree outlives the file now, so there is no lifetime to
    /// keep a caller from naming.
    #[must_use]
    pub fn source_file(&self) -> &'a SourceFile<'a> {
        self.source_file
    }

    /// JSDoc comments, keyed by the node they document.
    #[must_use]
    pub fn jsdoc(&self) -> &JSDocTable<'a> {
        &self.jsdoc
    }

    /// The run of node ids this file claimed in the program's node table.
    #[must_use]
    pub fn node_range(&self) -> Range<u32> {
        self.node_range.clone()
    }

    /// Whether `node` was parsed from this file.
    ///
    /// The question that did not exist under per-file identity, and that anything
    /// reading a `Span` against a file's text now has to ask: a node id names one
    /// node in the *program*, and only this file's text can be indexed by its
    /// span.
    #[must_use]
    pub fn contains(&self, node: NodeId) -> bool {
        self.node_range.contains(&node.as_u32())
    }

    /// What the file's `///`-directive preamble declared.
    ///
    /// The file loader reads these the same way it reads an `import`: a
    /// `path` reference names a file directly, a `types` reference goes through
    /// the resolver, and a `lib` reference names a built-in library.
    #[must_use]
    pub fn file_references(&self) -> &FileReferences {
        &self.file_references
    }

    /// What the scanner and parser objected to, in source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
