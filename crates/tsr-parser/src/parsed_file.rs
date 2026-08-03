//! An owned, sendable parse result.
//!
//! # The problem
//!
//! A parsed AST borrows from two things: the [`Arena`] its nodes live in and the
//! source text its identifiers point into. So `SourceFile<'a>` cannot outlive
//! either, and a bare `(Arena, String, SourceFile<'a>)` is self-referential —
//! which Rust will not let you name, let alone move.
//!
//! That matters the moment work is parallel. The unit of parallelism is a *file*:
//! a worker parses one and hands the result back. Handing it back requires an
//! owned value that is [`Send`].
//!
//! # The shape
//!
//! [`self_cell`] stores the owner (arena plus source) alongside the AST that
//! borrows from it, and hands out the borrow only through a closure. Moving the
//! cell moves the arena with everything pointing into it, so the references stay
//! valid. oxc solves the same problem the same way in
//! `oxc_type_checker::compiler::source_file` and `oxc_linter`.

use std::sync::Arc;

use tsr_ast::{NodeTable, SourceFile};
use tsr_core::Arena;
use tsr_diagnostics::Diagnostic;

/// What a parsed file owns: the arena its nodes live in, and its source text.
struct Owner {
    arena: Arena,
    source: String,
}

self_cell::self_cell! {
    struct Cell {
        owner: Owner,
        #[covariant]
        dependent: Ast,
    }
}

/// The parse output that borrows from [`Owner`].
struct Ast<'a> {
    source_file: &'a SourceFile<'a>,
}

/// A parsed file: source, arena, tree, and diagnostics as one owned value.
///
/// `Send`, so a worker can parse it and hand it back. Not `Sync`, because the
/// arena is not — but nothing needs shared concurrent access to one file.
pub struct ParsedFile {
    cell: Cell,
    /// Diagnostics in source order.
    diagnostics: Arc<[Diagnostic]>,
    /// Kind, span, and parent for every node.
    nodes: Arc<NodeTable>,
}

// SAFETY: the cell owns the arena together with the AST that borrows from it, and
// hands out no references that outlive it. Moving the cell moves the arena with
// its dependents, so every reference stays valid across the move. `Arena` is
// itself `Send` (see its safety note); `String` and the node tables are plain
// data.
//
// This is not `Sync`, and must not become so: the arena's bump pointer is a
// `Cell`.
unsafe impl Send for ParsedFile {}

impl ParsedFile {
    /// Parse `source`, taking ownership of it.
    #[must_use]
    pub fn parse(source: String) -> Self {
        let mut diagnostics = Vec::new();
        let mut nodes = NodeTable::new();

        let cell = Cell::new(Owner { arena: Arena::new(), source }, |owner| {
            // `source` is borrowed from the owner, so the AST's `&'a str`s point
            // into storage the cell keeps alive.
            let mut parser = crate::Parser::new(&owner.arena, &owner.source);
            let source_file = parser.parse_source_file();
            let (parsed_diagnostics, parsed_nodes) = parser.finish();
            diagnostics = parsed_diagnostics;
            nodes = parsed_nodes;
            Ast { source_file }
        });

        Self { cell, diagnostics: diagnostics.into(), nodes: Arc::new(nodes) }
    }

    /// The source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.cell.borrow_owner().source
    }

    /// Diagnostics from the scanner and parser, in source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Kind, span, and parent for every node.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        &self.nodes
    }

    /// Run `f` on the tree.
    ///
    /// The tree is reachable only through a closure: handing out `&SourceFile`
    /// directly would let a caller name a lifetime tied to storage the cell owns,
    /// which is the thing `self_cell` exists to prevent.
    pub fn with_ast<R>(&self, f: impl for<'a> FnOnce(&'a SourceFile<'a>) -> R) -> R {
        self.cell.with_dependent(|_owner, ast| f(ast.source_file))
    }

    /// How many statements the file has, without exposing the tree.
    ///
    /// Convenience for callers that only need a summary.
    #[must_use]
    pub fn statement_count(&self) -> usize {
        self.with_ast(|source_file| source_file.statements.len())
    }
}

impl std::fmt::Debug for ParsedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedFile")
            .field("bytes", &self.source().len())
            .field("nodes", &self.nodes.len())
            .field("diagnostics", &self.diagnostics.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_exposes_its_tree() {
        let parsed = ParsedFile::parse("const x = 1; const y = 2;".to_string());
        assert!(parsed.diagnostics().is_empty());
        assert_eq!(parsed.statement_count(), 2);
        assert!(parsed.nodes().len() > 2);
    }

    #[test]
    fn moves_between_threads_with_its_tree_intact() {
        // The point of the whole type: parse here, use there.
        let parsed = ParsedFile::parse("function f() { return 1; }".to_string());
        let handle = std::thread::spawn(move || parsed.statement_count());
        assert_eq!(handle.join().expect("thread panicked"), 1);
    }

    #[test]
    fn many_files_parse_concurrently() {
        // One arena per file, so there is nothing to contend on.
        let sources: Vec<String> =
            (0..64).map(|i| format!("const x{i} = {i}; function f{i}() {{}}")).collect();

        let handles: Vec<_> = sources
            .into_iter()
            .map(|source| std::thread::spawn(move || ParsedFile::parse(source)))
            .collect();

        for handle in handles {
            let parsed = handle.join().expect("thread panicked");
            assert!(parsed.diagnostics().is_empty());
            assert_eq!(parsed.statement_count(), 2);
        }
    }

    #[test]
    fn is_send() {
        const fn assert_send<T: Send>() {}
        assert_send::<ParsedFile>();
    }
}
