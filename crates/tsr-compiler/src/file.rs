//! One file in a program: its source, its tree, and its symbols, as one owned
//! value.
//!
//! # Why this is not `tsr_parser::ParsedFile`
//!
//! `ParsedFile` solves the same ownership problem for parsing: an arena, the
//! source text, and a tree that borrows from both, bundled by `self_cell` so the
//! whole thing can be moved and sent between threads. A program needs one more
//! borrower — the [`BindResult`], whose symbol names point into the source and
//! whose declarations point into the arena — and `tsr-parser` cannot hold it
//! without depending on `tsr-binder`, which would invert the layering.
//!
//! So the cell is rebuilt here with the binder's output inside it. That is
//! duplication, and the alternative was worse: a `ParsedFile` that knows about
//! symbols, or a `BindResult` stored beside the arena it borrows from, which
//! Rust will not let you name.
//!
//! # Binding is a second phase, as upstream has it
//!
//! Upstream parses during program construction and binds later, on demand
//! (`Program.BindSourceFiles`, gated on `file.IsBound()`). The tree here is built
//! once by `self_cell` and cannot gain a new dependent afterwards — so the
//! *storage* for the bind result is created up front, as an empty [`OnceCell`],
//! and filled by [`ProgramFile::bind`]. The phase separation upstream has is
//! preserved; only the allocation is eager.

use std::cell::OnceCell;

use tsr_ast::{NodeTable, SourceFile};
use tsr_binder::{BindResult, FileInfo};
use tsr_core::Arena;
use tsr_diagnostics::Diagnostic;
use tsr_parser::{FileReferences, JSDocTable, ScriptKind};
use tsr_path::Path;

/// What a file owns: the arena its nodes live in, its name, and its text.
struct Owner {
    arena: Arena,
    file_name: String,
    text: String,
}

// `not_covariant`, forced by the `OnceCell`: a cell is invariant in its
// parameter, so the dependent cannot be shrunk to a shorter lifetime and
// `self_cell` cannot hand out `borrow_dependent`. Everything reaches the
// contents through `with_dependent` instead, which is the access pattern the
// rest of this file already wanted.
self_cell::self_cell! {
    struct Cell {
        owner: Owner,
        #[not_covariant]
        dependent: Contents,
    }
}

/// Everything that borrows from [`Owner`].
struct Contents<'a> {
    source_file: &'a SourceFile<'a>,
    jsdoc: JSDocTable<'a>,
    /// Empty until [`ProgramFile::bind`]. See the module docs for why the
    /// storage exists before the value does.
    binder: OnceCell<BindResult<'a>>,
}

/// A file in a program.
///
/// `Send` for the reason `ParsedFile` is: the cell owns the arena together with
/// everything borrowing from it, so moving it moves them all. Not `Sync` — the
/// arena's bump pointer is a `Cell`, and so is the bind result's storage — which
/// is why [`crate::Program::bind_source_files`] hands each file to exactly one
/// worker.
pub struct ProgramFile {
    /// The file's identity, and the key it is stored under.
    path: Path,
    cell: Cell,
    diagnostics: Vec<Diagnostic>,
    nodes: NodeTable,
    file_references: FileReferences,
}

// SAFETY: as `tsr_parser::ParsedFile`, and for the same reason. The cell owns the
// arena alongside every value that borrows from it and hands out no reference
// outliving it, so moving the cell keeps every reference valid. `Arena` is
// `Send`; `String`, `NodeTable` and `Diagnostic` are plain data; `OnceCell<T>` is
// `Send` when `T` is. `self_cell` suppresses the automatic derive because the
// generated type holds a pointer into its own storage.
//
// This is not `Sync` and must not become so.
// See docs/adr/0011-unsafe-is-opt-in.md — the workspace denies `unsafe_code` and
// this is a named exception.
#[allow(unsafe_code)]
unsafe impl Send for ProgramFile {}

impl ProgramFile {
    /// Parse `text` as `file_name`, under `path`.
    ///
    /// The dialect follows the extension, as `ScriptKind::from_file_name` has it.
    #[must_use]
    pub fn parse(path: Path, file_name: String, text: String) -> Self {
        let script_kind = ScriptKind::from_file_name(&file_name);
        let mut diagnostics = Vec::new();
        let mut nodes = NodeTable::new();
        let mut file_references = FileReferences::default();

        let cell = Cell::new(Owner { arena: Arena::new(), file_name, text }, |owner| {
            let options = tsr_parser::ParseOptions { script_kind, ..Default::default() };
            let parsed = tsr_parser::parse_with_options(&owner.arena, &owner.text, options);
            diagnostics = parsed.diagnostics;
            nodes = parsed.nodes;
            file_references = parsed.file_references;
            Contents {
                source_file: parsed.source_file,
                jsdoc: parsed.jsdoc,
                binder: OnceCell::new(),
            }
        });

        Self { path, cell, diagnostics, nodes, file_references }
    }

    /// Bind the file, if it is not bound already.
    ///
    /// Upstream's `binder.BindSourceFile` behind `file.IsBound()`.
    pub fn bind(&mut self) {
        let nodes = &self.nodes;
        self.cell.with_dependent(|owner, contents| {
            contents.binder.get_or_init(|| {
                tsr_binder::bind(
                    contents.source_file,
                    nodes,
                    FileInfo { name: &owner.file_name, text: &owner.text },
                )
            });
        });
    }

    /// Whether [`ProgramFile::bind`] has run (`file.IsBound()`).
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.cell.with_dependent(|_owner, contents| contents.binder.get().is_some())
    }

    /// The file's canonical identity.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file name as it was given, before canonicalisation.
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.cell.borrow_owner().file_name
    }

    /// The source text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.cell.borrow_owner().text
    }

    /// Kind, span, and parent for every node.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        &self.nodes
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

    /// Run `f` on the tree and the file's symbols.
    ///
    /// A closure rather than a getter for the reason `ParsedFile::with_ast` gives:
    /// handing out a `&SourceFile` would let a caller name a lifetime tied to
    /// storage the cell owns.
    ///
    /// # Panics
    ///
    /// If the file has not been bound. [`crate::Program`] binds every file before
    /// handing one out, so this is a programming error rather than a state a
    /// caller can reach.
    pub fn with_bound<R>(
        &self,
        f: impl for<'a> FnOnce(&'a SourceFile<'a>, &'a BindResult<'a>) -> R,
    ) -> R {
        self.cell.with_dependent(|_owner, contents| {
            let binder = contents.binder.get().expect("the file is bound before it is handed out");
            f(contents.source_file, binder)
        })
    }

    /// Run `f` on the tree alone, bound or not.
    pub fn with_ast<R>(&self, f: impl for<'a> FnOnce(&'a SourceFile<'a>) -> R) -> R {
        self.cell.with_dependent(|_owner, contents| f(contents.source_file))
    }

    /// Run `f` on the tree and its JSDoc.
    pub fn with_ast_and_jsdoc<R>(
        &self,
        f: impl for<'a> FnOnce(&'a SourceFile<'a>, &'a JSDocTable<'a>) -> R,
    ) -> R {
        self.cell.with_dependent(|_owner, contents| f(contents.source_file, &contents.jsdoc))
    }
}

impl std::fmt::Debug for ProgramFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramFile")
            .field("path", &self.path)
            .field("bytes", &self.text().len())
            .field("nodes", &self.nodes.len())
            .field("bound", &self.is_bound())
            .finish_non_exhaustive()
    }
}
