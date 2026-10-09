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
    /// Lives inside the cell rather than beside it, because it borrows JSDoc
    /// nodes from the arena — unlike the diagnostics and node table, which are
    /// owned data and can be lifted out.
    jsdoc: crate::JSDocTable<'a>,
    /// Inside the cell for the same reason as `jsdoc`: it holds `Node<'a>`
    /// values borrowed from the arena, so it cannot be lifted out beside the
    /// node table. See [`tsr_ast::NodeMap`].
    node_map: tsr_ast::NodeMap<'a>,
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
// The workspace denies `unsafe_code`; this is one of two exceptions, and the
// only one outside the arena. `self_cell` produces a type holding a pointer into
// its own storage, which suppresses the automatic `Send` derive even though every
// part is `Send`. See docs/adr/0011-unsafe-is-opt-in.md.
#[allow(unsafe_code)]
unsafe impl Send for ParsedFile {}

impl ParsedFile {
    /// Parse `source` as TypeScript, taking ownership of it.
    ///
    /// Use [`ParsedFile::parse_with_script_kind`] for `.tsx` and `.json`, where the
    /// dialect changes what the text means.
    #[must_use]
    pub fn parse(source: String) -> Self {
        Self::parse_with_script_kind(source, crate::ScriptKind::TypeScript)
    }

    /// Parse `source` in a given dialect, taking ownership of it.
    ///
    /// The dialect is not cosmetic: in `.tsx` a leading `<` opens JSX, and in `.ts`
    /// the same character opens a type assertion. Parsing a `.tsx` file as
    /// TypeScript therefore produces a tree that is wrong rather than merely
    /// different, and any analysis run over it reports diagnostics about syntax
    /// that is not there. Callers working from a file name should pair this with
    /// [`ScriptKind::from_file_name`](crate::ScriptKind::from_file_name).
    #[must_use]
    pub fn parse_with_script_kind(source: String, script_kind: crate::ScriptKind) -> Self {
        Self::parse_with_options(source, crate::ParseOptions { script_kind, ..Default::default() })
    }

    /// Parse privately using the same options as program-owned parsing.
    #[must_use]
    pub fn parse_with_options(source: String, options: crate::ParseOptions) -> Self {
        let mut diagnostics = Vec::new();
        let mut nodes = NodeTable::new();

        let cell = Cell::new(Owner { arena: Arena::new(), source }, |owner| {
            // `source` is borrowed from the owner, so the AST's `&'a str`s point
            // into storage the cell keeps alive.
            let mut parser = crate::Parser::with_options(&owner.arena, &owner.source, options);
            // JSON is not a dialect of the statement grammar: a document is one
            // value, so it needs its own entry point rather than a flag.
            let source_file = if options.script_kind == crate::ScriptKind::Json {
                parser.parse_json_text()
            } else {
                parser.parse_source_file()
            };
            let (parsed_diagnostics, parsed_nodes, jsdoc, node_map) = parser.finish();
            diagnostics = parsed_diagnostics;
            // `processPragmasIntoFields`' reports (`crate::pragma_diagnostics`).
            crate::pragma_diagnostics::append_pragma_diagnostics(
                &crate::pragma::parse_file_references(&owner.source),
                &mut diagnostics,
            );
            nodes = parsed_nodes;
            Ast { source_file, jsdoc, node_map }
        });

        Self { cell, diagnostics: diagnostics.into(), nodes: Arc::new(nodes) }
    }

    /// Publish into the caller's arena and ordered program tables. The private
    /// owner remains alive throughout copying and can be dropped immediately
    /// afterwards. No reference in the result borrows from this owner.
    pub fn publish<'a>(
        &self,
        arena: &'a Arena,
        source: &'a str,
        nodes: &mut NodeTable,
        node_map: &mut tsr_ast::NodeMap<'a>,
    ) -> crate::ParsedInto<'a> {
        use tsr_ast::publication::{Publication, Publish};
        assert_eq!(source, self.source(), "publication source differs from parsed source");
        let node_range = nodes.append_relocated(&self.nodes);
        self.cell.with_dependent(|owner, ast| {
            let mut publication =
                Publication::new(arena, &owner.source, source, node_range.start, self.nodes.len());
            publication.finish(&ast.node_map, node_map);
            let source_file = ast.source_file.publish(&mut publication);
            let jsdoc = ast.jsdoc.publish(&mut publication);
            crate::ParsedInto {
                source_file,
                diagnostics: self.diagnostics.to_vec(),
                jsdoc,
                file_references: crate::pragma::parse_file_references(source),
                node_range,
            }
        })
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

    /// Run `f` over the tree and its node map together.
    ///
    /// The map is how a consumer holding only a [`NodeId`](tsr_ast::NodeId) —
    /// a symbol's declaration, or a node's parent — reads the node's fields.
    pub fn with_ast_and_nodes<R>(
        &self,
        f: impl for<'a> FnOnce(&'a SourceFile<'a>, &'a tsr_ast::NodeMap<'a>) -> R,
    ) -> R {
        self.cell.with_dependent(|_owner, ast| f(ast.source_file, &ast.node_map))
    }

    /// Run `f` over the tree and its JSDoc together.
    ///
    /// Separate from [`ParsedFile::with_ast`] so the common caller, which does
    /// not care about comments, keeps the simpler signature.
    pub fn with_ast_and_jsdoc<R>(
        &self,
        f: impl for<'a> FnOnce(&'a SourceFile<'a>, &'a crate::JSDocTable<'a>) -> R,
    ) -> R {
        self.cell.with_dependent(|_owner, ast| f(ast.source_file, &ast.jsdoc))
    }

    /// Run `f` over the tree and the arena it lives in.
    ///
    /// For callers that go on to *allocate* against the tree's lifetime — the
    /// binder interns the few symbol names that are not source borrows.
    pub fn with_ast_and_arena<R>(
        &self,
        f: impl for<'a> FnOnce(&'a SourceFile<'a>, &'a Arena) -> R,
    ) -> R {
        self.cell.with_dependent(|owner, ast| f(ast.source_file, &owner.arena))
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
