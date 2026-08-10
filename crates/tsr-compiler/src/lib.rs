//! The `Program`: a set of files compiled together under one set of options.
//!
//! Ported from `internal/compiler/program.go` at the pinned commit — the part of
//! it that matters before there is a checker or an emitter. Upstream's `Program`
//! is 2,232 lines, and most of that is emit, project references, redirect
//! deduplication, and diagnostic plumbing. What is here is `processedFiles`
//! (`files`, `filesByPath`) and `BindSourceFiles`.
//!
//! # Why this exists before module resolution
//!
//! Until now every file was parsed and bound *alone*. That was not a
//! simplification, it was the truth: there was no object that held more than one
//! file, so a symbol declared in `a.ts` was genuinely unreachable from `b.ts` and
//! the conformance harness said so in as many words. A `Program` is that object.
//! It is also what a lib file is loaded *into*, which is why "we cannot load
//! `lib.d.ts`" was never a separate problem — see
//! [ADR-0017](../../../docs/adr/0017-program-before-tsconfig.md).
//!
//! # What it deliberately does not do
//!
//! - **It does not read tsconfig.json.** Options arrive as a
//!   [`CompilerOptions`], which is what upstream's `program.go` takes — it
//!   imports `internal/core` and not `internal/tsoptions`.
//! - **It does not merge globals — the *binder* does, and this note used to say
//!   otherwise.** Until [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)
//!   landed (`454478a`, `fa16ae4`) this crate held one `BindResult` per file and
//!   no file could see another's names; the sentence here read "this is the
//!   reason a program that holds `lib.es5.d.ts` still cannot answer `Array`".
//!   **That has been false since the widening.** `bind_into` accumulates every
//!   file into one `BindResult`, and `Binder::merge_globals`
//!   (`crates/tsr-binder/src/binder.rs:511`) folds each *script* file's
//!   top-level locals into `BindResult::globals` — upstream's
//!   `initializeChecker` loop (`internal/checker/checker.go:1300`-`:1328`), in
//!   the binder rather than the checker because that is where this port's
//!   symbol tables live. ADR-0034's own outcome section measured the effect:
//!   lib names unresolved in type position went 3,209 to **3**.
//!
//!   What is genuinely still missing is one step further in: upstream's
//!   `mergeGlobalSymbol` (`checker.go:1386`) calls `mergeSymbol` (`:14146`),
//!   which unions the *declarations* of two symbols sharing a name. This port
//!   takes **first-in-wins** (`entry().or_insert()`) and says so where it does
//!   it. That is not a small residue for lib types specifically —
//!   `interface Array<T>` is declared across **8** bundled lib files and
//!   `String` across 10 — so a global resolves, and carries only the members of
//!   whichever file bound first. `T[]` therefore answers; `a.flat()` does not.
//!
//! # Two ways in
//!
//! [`Program::new`] takes a file list. [`Program::from_root_files`] takes *root*
//! files and a host, and runs [`loader::FileLoader`] to discover the rest —
//! imports, `/// <reference />` directives, and the bundled `lib.*.d.ts`. The
//! first is what the conformance corpus supplies today; the second is what `tsc`
//! does.

mod file;
pub mod loader;

use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;
use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo};
use tsr_core::{Arena, CompilerOptions};
use tsr_path::{Path, to_path};

pub use file::ProgramFile;
pub use loader::{FileLoader, LoadOptions, LoadedFiles, RequestKind, ResolutionRequest};

/// How a program is constructed (`compiler.ProgramOptions`).
#[derive(Debug, Clone)]
pub struct ProgramOptions {
    /// The files to compile, as `(file name, text)`.
    ///
    /// Given rather than discovered: there is no resolver yet, and the
    /// conformance corpus supplies its files directly.
    pub files: Vec<(String, String)>,
    /// The options every file is compiled under.
    pub compiler_options: CompilerOptions,
    /// The directory a relative file name is resolved against.
    pub current_directory: String,
    /// Whether the host distinguishes `A.ts` from `a.ts`.
    ///
    /// A property of the file system, not of TypeScript, which is why it is a
    /// program input rather than a compiler option.
    pub use_case_sensitive_file_names: bool,
}

impl Default for ProgramOptions {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            compiler_options: CompilerOptions::default(),
            current_directory: "/".to_string(),
            // Upstream's test host is case-sensitive, and so is the corpus: two
            // cases differing only in case are two files.
            use_case_sensitive_file_names: true,
        }
    }
}

/// What one file's `import "x"` resolved to.
///
/// The value side of upstream's `p.resolvedModules[path]`
/// (`internal/compiler/program.go:456`), which is a
/// `module.ModeAwareCache[*module.ResolvedModule]` — a map keyed by
/// `{Name, Mode}`. **This port keys on the name alone**, for the reason recorded
/// in [ADR-0041](../../../docs/adr/0041-the-checker-asks-its-program-for-a-module.md):
/// the checker's side of the seam has no way to compute the mode of a usage
/// location (upstream's `getModeForUsageLocation`,
/// `internal/compiler/fileloader.go:726`, reads the specifier's parent syntax
/// and the file's `SourceFileMetaData`, neither of which reaches the checker).
///
/// Collapsing the key is only safe when the collapsed entries agree, so
/// disagreement is represented rather than resolved by a tie-break. The mode
/// itself is **not** dropped from the port: [`loader::ResolutionRequest`] keeps
/// it, and this map is derived from those requests.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModuleResolution {
    /// Every request for this specifier text in this file resolved here.
    To(Path),
    /// Every request for it failed to resolve — upstream's
    /// `!resolvedModule.IsResolved()`.
    Unresolved,
    /// Two requests for the same specifier text in the same file, under
    /// different modes, answered differently.
    ///
    /// Real, not theoretical: under `module: node16` a file containing both
    /// `import "./b"` and `import("./b")` resolves the first as `CommonJS` and
    /// the second as `ESNext`, and only the first finds `/b.ts`. Since this port
    /// cannot tell which usage is asking, it answers **nothing** — the same
    /// discipline `errorType` encodes everywhere else here. Inventing the
    /// majority answer would be a confidently wrong type rather than a gap.
    Conflicting,
}

/// A set of files compiled together.
///
/// Lifetime-parameterised because the program borrows the arena its files were
/// parsed into, which the **caller** owns (ADR-0034, "Who owns the arena"). The
/// accepted cost is that a `Program` cannot be returned from a function that
/// created its own arena; the escape hatch, if a language service ever needs
/// one, is a `self_cell` around the arena at the top level — one
/// self-referential type instead of one per file.
#[derive(Debug)]
pub struct Program<'a> {
    options: CompilerOptions,
    /// In the order they were given, which is the order diagnostics are
    /// reported in.
    files: Vec<ProgramFile<'a>>,
    /// Canonical path to its index in `files`.
    ///
    /// Upstream keys `filesByPath` by `*ast.SourceFile`; an index is the same
    /// thing without a second borrow of the file.
    files_by_path: FxHashMap<Path, usize>,
    /// The directory a relative file name is resolved against, and whether the
    /// host distinguishes `A.ts` from `a.ts`.
    ///
    /// Both are what `Program.toPath` reads
    /// (`internal/compiler/program.go:1830`), and they are stored for the same
    /// reason upstream stores them: canonicalising a name is a property of the
    /// *program*, not of the caller doing the asking. See
    /// [`Program::source_file`].
    current_directory: String,
    use_case_sensitive_file_names: bool,

    /// How many leading entries of `files` are bundled lib files.
    ///
    /// Upstream keeps `libFiles` as a separate slice and concatenates it in
    /// front (`filesparser.go:518`); the boundary is kept rather than the two
    /// slices, because everything else about a lib file is ordinary.
    lib_file_count: usize,
    /// What the **loader** could not resolve — see
    /// [`loader::LoaderDiagnostic`] and `checker-notes-diag2.md` §240. Kept on
    /// the program because that is where upstream keeps them
    /// (`program.fileProcessingDiagnostics`): they belong to no checker and to
    /// no binder, and before §240 this port had nowhere to put them at all.
    loader_diagnostics: Vec<loader::LoaderDiagnostic>,
    /// Kind, span and parent for every node of **every** file
    /// ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)).
    nodes: NodeTable,
    /// The typed node behind each id, over the same numbering.
    node_map: NodeMap<'a>,
    /// Every symbol of every bound file, in one store.
    ///
    /// The `SourceFile` node of each file, by its id.
    ///
    /// The reverse of `files[i].source_file().node_id`. It exists because the
    /// checker's currency for "which file" is a `NodeId` — under
    /// [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)
    /// one node table spans the program, so a `SourceFile`'s id names a file
    /// exactly as upstream's `*ast.SourceFile` pointer does — while the
    /// resolution cache is keyed by [`Path`], which is upstream's key. This is
    /// the translation between the two identity spaces, built once rather than
    /// scanned per import.
    files_by_source_file: FxHashMap<NodeId, usize>,
    /// `import specifier -> the file it resolved to`, per importing file.
    ///
    /// Upstream's `p.resolvedModules` (`internal/compiler/program.go:456`), the
    /// map `Program.GetResolvedModule` (`:521`) reads and the checker calls
    /// through its `Program` interface (`internal/checker/checker.go:558`) from
    /// `resolveExternalModule` (`checker.go:15207`).
    ///
    /// **Populated only by [`Program::from_root_files`].** A program built from
    /// a file list ([`Program::new`]) ran no resolver, so it has no resolutions
    /// and every lookup answers `None` — which is a gap, not a wrong answer.
    resolved_modules: FxHashMap<Path, FxHashMap<String, ModuleResolution>>,
    /// One rather than one per file, which is the whole of the widening: a
    /// `SymbolId` names one symbol across the program, and its declarations
    /// index `nodes`, so a symbol from another file can be handed to the checker
    /// without reading the wrong file's declarations.
    binder: BindResult<'a>,
    /// How many leading entries of `files` have been bound.
    ///
    /// Upstream's per-file `file.IsBound()`, coarsened to a prefix because one
    /// `SymbolStore` is filled by one sequential accumulation. See
    /// [`Program::bind_source_files`].
    bound_file_count: usize,
}

impl<'a> Program<'a> {
    /// Parse every file into `arena` and bind it.
    ///
    /// Named for the arena rather than `new` because the arena is the
    /// load-bearing argument: it is what every file's tree, name, text and
    /// symbol borrows from, and its extent *is* the compilation (ADR-0034). A
    /// `new` that took it as a first parameter would read as an implementation
    /// detail; it is the contract.
    ///
    /// Upstream splits parse and bind — `NewProgram` parses, `BindSourceFiles`
    /// binds on demand — and so does this, one line apart. They are separate
    /// because binding is the expensive half and a consumer that only wants
    /// syntax should not pay for it; [`Program::parse`] is that entry point.
    #[must_use]
    pub fn in_arena(arena: &'a Arena, options: ProgramOptions) -> Self {
        let mut program = Self::parse(arena, options);
        program.bind_source_files(arena);
        program
    }

    /// Parse every file into `arena`, without binding.
    ///
    /// **Sequential, where this used to be a rayon `into_par_iter`.** One arena
    /// and one node table cannot be filled from several threads, and the ids
    /// have to continue the numbering in a defined order — the same
    /// serialisation ADR-0034 records for binding, arriving one phase earlier.
    /// Parallelism moves out to the caller, which is where the conformance
    /// harness already has it: a case per worker, an arena per case.
    #[must_use]
    pub fn parse(arena: &'a Arena, options: ProgramOptions) -> Self {
        let ProgramOptions {
            files,
            compiler_options,
            current_directory,
            use_case_sensitive_file_names,
        } = options;

        let mut nodes = NodeTable::new();
        let mut node_map = NodeMap::new();
        let mut parsed: Vec<ProgramFile<'a>> = Vec::with_capacity(files.len());
        for (file_name, text) in files {
            let path = to_path(&file_name, &current_directory, use_case_sensitive_file_names);
            // Copied into the arena for the reason the loader copies: the
            // symbol store outlives any per-file storage. See
            // `ProgramFile`'s module docs.
            let file_name: &'a str = arena.alloc_str(&file_name);
            let text: &'a str = arena.alloc_str(&text);
            let parse_options = tsr_parser::ParseOptions {
                script_kind: tsr_parser::ScriptKind::from_file_name(file_name),
                ..Default::default()
            };
            let into =
                tsr_parser::parse_into(arena, text, parse_options, &mut nodes, &mut node_map);
            // Upstream's parser stamps `NodeFlagsJavaScriptFile` from its
            // `ScriptKind`; this parser never sees the file name (ADR-0016),
            // so the program — the one place that holds both the name and the
            // table — stamps the root. `checker-notes-assign.md` §11.1 is the
            // consumer that found the flag declared and set by nothing.
            let lowered = file_name.to_ascii_lowercase();
            if [".js", ".jsx", ".cjs", ".mjs"].iter().any(|ext| lowered.ends_with(ext))
                && let Some(root) = tsr_ast::Node::SourceFile(into.source_file).node_id()
            {
                nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
            }
            parsed.push(ProgramFile::new(path, file_name, text, into));
        }

        // First spelling wins, as upstream's `filesByPath` does: a file added
        // twice under two spellings of one path is one file, and the later one
        // is a redirect upstream tracks and we do not yet.
        let mut files_by_path = FxHashMap::default();
        for (index, file) in parsed.iter().enumerate() {
            files_by_path.entry(file.path().clone()).or_insert(index);
        }

        let files_by_source_file = source_file_index(&parsed);
        Self {
            options: compiler_options,
            files: parsed,
            files_by_path,
            files_by_source_file,
            // A file list is not a resolution: nothing here asked a resolver
            // anything, so there is nothing to record. See `resolved_modules`.
            resolved_modules: FxHashMap::default(),
            current_directory,
            use_case_sensitive_file_names,
            lib_file_count: 0,
            loader_diagnostics: Vec::new(),
            nodes,
            node_map,
            binder: BindResult::empty(),
            bound_file_count: 0,
        }
    }

    /// Discover the program's files from its roots, then parse and bind them.
    ///
    /// Upstream's `NewProgram`, which calls `processAllProgramFiles`
    /// (`internal/compiler/program.go`) and takes the file list it returns.
    /// [`Program::new`] is the other half of the same function — the one that is
    /// handed a list — and the two meet here.
    ///
    /// The files come out of the loader **already parsed**: `parseTask.load`
    /// parses each one to find its imports, so parsing again would be a second
    /// pass over the same text. That is not a micro-optimisation once lib files
    /// are in the picture — `lib.dom.d.ts` alone is 2.3 MB.
    ///
    /// Binding still happens here rather than in the loader, because upstream
    /// keeps that split: `NewProgram` parses and `BindSourceFiles` binds later,
    /// on demand.
    #[must_use]
    pub fn from_root_files(
        arena: &'a Arena,
        host: &dyn tsr_module::types::ResolutionHost,
        options: loader::LoadOptions,
    ) -> Self {
        let compiler_options = options.compiler_options.clone();
        let loaded = loader::FileLoader::load(arena, host, options);

        let mut files_by_path = FxHashMap::default();
        for (index, file) in loaded.files.iter().enumerate() {
            files_by_path.entry(file.path().clone()).or_insert(index);
        }
        let files_by_source_file = source_file_index(&loaded.files);
        let current_directory = host.current_directory().to_string();
        let use_case_sensitive_file_names = host.fs().use_case_sensitive_file_names();
        let resolved_modules =
            resolved_modules(&loaded.requests, &current_directory, use_case_sensitive_file_names);

        let mut program = Self {
            options: compiler_options,
            files: loaded.files,
            files_by_path,
            files_by_source_file,
            resolved_modules,
            // From the host, which is where the loader took them from too — so
            // a name looked up afterwards canonicalises exactly as the path it
            // is being compared against did.
            current_directory,
            use_case_sensitive_file_names,
            lib_file_count: loaded.lib_file_count,
            loader_diagnostics: loaded.loader_diagnostics,
            nodes: loaded.nodes,
            node_map: loaded.node_map,
            binder: BindResult::empty(),
            bound_file_count: 0,
        };
        program.bind_source_files(arena);
        program
    }

    /// Bind every file that is not bound (`Program.BindSourceFiles`).
    ///
    /// **Sequential, where this used to be a rayon `par_iter_mut`.**
    /// `bind_into` accumulates into one `SymbolStore`, so a program binds in one
    /// pass, in file order — libs first, because a global interface declared in
    /// several files merges in the order the files were added. Upstream binds in
    /// parallel and can, because its symbols are pointers with no shared
    /// allocator between them; ADR-0034 records the trade and the escape hatch
    /// (bind per file into its own store, merge with a `SymbolId` offset).
    ///
    /// Idempotent, as upstream's `file.IsBound()` guard makes it: only the files
    /// past `bound_file_count` are bound, so calling this twice binds nothing
    /// the second time.
    pub fn bind_source_files(&mut self, arena: &'a Arena) {
        for index in self.bound_file_count..self.files.len() {
            let file = &self.files[index];
            let previous = std::mem::replace(&mut self.binder, BindResult::empty());
            let jsdoc: Vec<_> = file.jsdoc().iter().collect();
            self.binder = tsr_binder::bind_into_with_jsdoc(
                previous,
                arena,
                file.source_file(),
                &self.nodes,
                FileInfo { name: file.file_name(), text: file.text() },
                &jsdoc,
            );
        }
        self.bound_file_count = self.files.len();
    }

    /// The options every file is compiled under.
    #[must_use]
    pub fn compiler_options(&self) -> &CompilerOptions {
        &self.options
    }

    /// Kind, span and parent for every node of every file.
    ///
    /// One table for the program, not one per file: see
    /// [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md).
    /// A `Span` inside it is still an offset into *its own* file's text, which
    /// [`ProgramFile::contains`] is how you find.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        &self.nodes
    }

    /// The typed node behind each id.
    #[must_use]
    pub fn node_map(&self) -> &NodeMap<'a> {
        &self.node_map
    }

    /// Every symbol of every bound file, in one store.
    ///
    /// This is what a checker over a whole program takes, together with
    /// [`Program::nodes`] and [`Program::node_map`].
    #[must_use]
    pub fn binder(&self) -> &BindResult<'a> {
        &self.binder
    }

    /// How many of [`Program::source_files`] have been bound.
    ///
    /// Upstream's `file.IsBound()` per file; a prefix here because binding is
    /// one sequential accumulation into one store.
    #[must_use]
    pub fn bound_file_count(&self) -> usize {
        self.bound_file_count
    }

    /// Every file, in the order given (`Program.SourceFiles`).
    ///
    /// Lib files lead, when there are any. See [`Program::lib_files`].
    #[must_use]
    pub fn source_files(&self) -> &[ProgramFile<'a>] {
        &self.files
    }

    /// The bundled `lib.*.d.ts` this program loaded, in load order.
    ///
    /// Load order is not incidental: a global interface declared in several libs
    /// merges in the order the files were added, so `lib.es5.d.ts`'s `Array`
    /// comes before `lib.es2015.iterable.d.ts`'s additions to it.
    #[must_use]
    pub fn lib_files(&self) -> &[ProgramFile<'a>] {
        &self.files[..self.lib_file_count]
    }

    /// Everything that is not a lib file, in the order the loader reached it.
    #[must_use]
    pub fn root_and_referenced_files(&self) -> &[ProgramFile<'a>] {
        &self.files[self.lib_file_count..]
    }

    /// The file with this canonical path (`Program.GetSourceFileByPath`).
    #[must_use]
    pub fn source_file_by_path(&self, path: &Path) -> Option<&ProgramFile<'a>> {
        self.files_by_path.get(path).map(|index| &self.files[*index])
    }

    /// The file this *file name* denotes (`Program.GetSourceFile`).
    ///
    /// Canonicalises the name first, so `./a.ts` and `/a.ts` find the same file.
    /// One line, as upstream's is: `p.toPath(filename)` then
    /// `GetSourceFileByPath` (`internal/compiler/program.go:1834`).
    ///
    /// # It used to guess, and the guess was wrong
    ///
    /// This took a `current_directory` argument and stored neither it nor the
    /// case sensitivity, recovering the latter by trying the case-sensitive
    /// conversion and then the case-insensitive one — on the reasoning that a
    /// path which round-trips unchanged was built that way. Probed rather than
    /// argued (`bd tsr-q89`): a case-**sensitive** program holding `a.ts`,
    /// asked for `A.ts`, missed on `/A.ts` and then *hit* on the fallback's
    /// `/a.ts`. It answered a name it should not have, for every file whose
    /// name is already lowercase — which is most of them.
    ///
    /// Pre-existing, and made worse by the identity widening rather than caused
    /// by it: one `NodeTable` now spans the program and a `Span` is an offset
    /// into one file's text ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)),
    /// so a caller that looked a unit up by name and then read spans against a
    /// *different* unit's text got plausible positions from the wrong file
    /// rather than anything that failed.
    #[must_use]
    pub fn source_file(&self, file_name: &str) -> Option<&ProgramFile<'a>> {
        self.source_file_by_path(&self.to_path(file_name))
    }

    /// Diagnostics the **loader** produced, in walk order.
    ///
    /// `program.fileProcessingDiagnostics` (`compiler/program.go`). Separate
    /// from the binder's and the checker's because they are produced before
    /// either runs — see `checker-notes-diag2.md` §240.
    #[must_use]
    pub fn loader_diagnostics(&self) -> &[loader::LoaderDiagnostic] {
        &self.loader_diagnostics
    }

    /// The file `import "<specifier>"` in `importing_file` resolved to.
    ///
    /// `Program.GetResolvedModule` (`internal/compiler/program.go:521`)
    /// composed with `Program.GetSourceFileForResolvedModule` (`:1839`), which
    /// is how upstream's `resolveExternalModule` uses them — `checker.go:15207`
    /// and `:15216`, two calls with the intervening
    /// `GetResolutionDiagnostic` filter that this port has no diagnostics for
    /// yet.
    ///
    /// Both ids are **`SourceFile` node ids**. That is the checker's currency
    /// for "which file": under
    /// [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)
    /// one node table spans the program, so a `SourceFile`'s id names a file as
    /// unambiguously as upstream's `*ast.SourceFile` pointer. The [`Path`] the
    /// cache is keyed by never crosses the seam.
    ///
    /// `None` covers four distinct situations, all of which must answer
    /// `errorType` and none of which may answer a guess:
    ///
    /// | why | upstream |
    /// |---|---|
    /// | this file never imported that specifier | key miss on `p.resolvedModules` |
    /// | it did, and resolution failed | `!resolvedModule.IsResolved()` |
    /// | it resolved to a file the program does not hold | `GetSourceFileForResolvedModule` returns nil |
    /// | two modes resolved it differently | *no counterpart* — see [`ModuleResolution::Conflicting`] |
    ///
    /// Only the last is a divergence, and it is a gap rather than a wrong
    /// answer.
    #[must_use]
    pub fn resolved_module(&self, importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let index = *self.files_by_source_file.get(&importing_file)?;
        let resolution = self.resolved_modules.get(self.files[index].path())?.get(specifier)?;
        let ModuleResolution::To(target) = resolution else { return None };
        // `GetSourceFileForResolvedModule`: a resolution that named a file the
        // program does not hold answers nothing. Membership is a lookup in
        // `files_by_path`, and both sides of it were canonicalised by
        // `to_path` under this program's own `current_directory` and case
        // sensitivity — the target here at construction, the members when they
        // were added — so a case-differing pair cannot alias. That is only true
        // because `Program::source_file` stopped recovering case sensitivity by
        // trying both conversions (`bd tsr-q89`); see its doc comment.
        self.source_file_by_path(target)?.source_file().node_id
    }

    /// Did the resolver name a file for this specifier, whether or not the
    /// program holds it?
    ///
    /// `Program.GetResolvedModule(...).IsResolved()`. The difference from
    /// [`Program::resolved_module`] is the `GetSourceFileForResolvedModule`
    /// membership hop that method ends with: an untyped `node_modules` package
    /// under `allowJs: false`, or a `.tsx` reached without `--jsx`, resolves
    /// here and is absent there. Upstream reports a different diagnostic code
    /// for each, which is the only reason the distinction is exposed —
    /// see [`tsr_checker::resolution::ModuleHost::module_resolution_found`].
    #[must_use]
    pub fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        let Some(&index) = self.files_by_source_file.get(&importing_file) else { return false };
        let Some(resolutions) = self.resolved_modules.get(self.files[index].path()) else {
            return false;
        };
        matches!(resolutions.get(specifier), Some(ModuleResolution::To(_)))
    }

    /// A file name canonicalised the way this program canonicalises
    /// (`Program.toPath`, `internal/compiler/program.go:1830`).
    #[must_use]
    pub fn to_path(&self, file_name: &str) -> Path {
        to_path(file_name, &self.current_directory, self.use_case_sensitive_file_names)
    }

    /// Every parse diagnostic in the program, file by file.
    #[must_use]
    pub fn syntactic_diagnostics(&self) -> Vec<(&Path, &tsr_diagnostics::Diagnostic)> {
        self.files
            .iter()
            .flat_map(|file| file.diagnostics().iter().map(move |d| (file.path(), d)))
            .collect()
    }
}

/// `Program` is what the checker's module seam is implemented by
/// (`internal/compiler/program.go`'s `*Program` satisfying
/// `checker.Program`, `internal/checker/checker.go:547`).
///
/// The dependency runs **this way round on purpose**: `tsr-compiler` names
/// `tsr-checker`, as upstream's `internal/compiler` imports
/// `internal/checker`. The reverse would become a cycle as soon as the compiler
/// drives a check traversal for diagnostics
/// ([ADR-0040](../../../docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)),
/// which is why the trait is declared over there and satisfied here rather than
/// the checker taking a `Program`. See
/// [ADR-0041](../../../docs/adr/0041-the-checker-asks-its-program-for-a-module.md).
impl tsr_checker::resolution::ModuleHost for Program<'_> {
    fn resolved_module(&self, importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        Program::resolved_module(self, importing_file, specifier)
    }

    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        Program::module_resolution_found(self, importing_file, specifier)
    }
    fn file_path(&self, file: NodeId) -> Option<String> {
        let &index = self.files_by_source_file.get(&file)?;
        Some(self.files[index].file_name().to_string())
    }

    fn resolved_module_path(&self, importing_file: NodeId, specifier: &str) -> Option<String> {
        let &index = self.files_by_source_file.get(&importing_file)?;
        let resolutions = self.resolved_modules.get(self.files[index].path())?;
        match resolutions.get(specifier) {
            Some(ModuleResolution::To(path)) => Some(path.to_string()),
            _ => None,
        }
    }
    fn jsdoc_template_parameters(&self, declaration: NodeId) -> Vec<NodeId> {
        // §110: linear over files, then over each file's (host, docs) rows —
        // the table is small and the call is bake-time-only.
        let mut parameters = Vec::new();
        for file in self.root_and_referenced_files() {
            let docs = file.jsdoc().get(declaration);
            {
                for doc in docs {
                    for tag in doc.tags {
                        if let tsr_ast::JSDocTag::JSDocTemplateTag(template) = tag {
                            parameters.extend(
                                template
                                    .type_parameters
                                    .iter()
                                    .filter_map(|parameter| parameter.node_id),
                            );
                        }
                    }
                }
            }
        }
        parameters
    }

    fn jsx_factory_namespace(&self, file: tsr_ast::NodeId) -> Option<String> {
        self.root_and_referenced_files()
            .iter()
            .find(|candidate| candidate.source_file().node_id == Some(file))?
            .file_references()
            .jsx_factory_namespace
            .clone()
    }

    fn is_declaration_file(&self, file: tsr_ast::NodeId) -> bool {
        self.root_and_referenced_files()
            .iter()
            .find(|candidate| candidate.source_file().node_id == Some(file))
            .is_some_and(|candidate| tsr_binder::is_declaration_file(candidate.file_name()))
    }
}

/// Each file's `SourceFile` node id to its index in `files`.
///
/// A file whose `SourceFile` carries no id is skipped rather than panicked on:
/// the field is `Option<NodeId>` because a node is registered a moment after it
/// is allocated, and a finished tree has `Some` everywhere. Skipping means such
/// a file is simply never found by id, which is a gap.
fn source_file_index(files: &[ProgramFile<'_>]) -> FxHashMap<NodeId, usize> {
    let mut index = FxHashMap::default();
    for (position, file) in files.iter().enumerate() {
        if let Some(id) = file.source_file().node_id {
            index.insert(id, position);
        }
    }
    index
}

/// Build `p.resolvedModules` from what the loader recorded.
///
/// Upstream fills the map as it resolves (`fileloader.go`'s
/// `resolveImportsAndModuleAugmentations` writes
/// `t.resolutionsInFile`, gathered into `p.resolvedModules`); this port gathers
/// it afterwards from [`loader::ResolutionRequest`], which carries the same
/// `(containing file, name, mode)` key plus the answer.
///
/// **Only `RequestKind::Module`.** Upstream keeps type reference directives in a
/// separate map, `p.typeResolutionsInFile` (`program.go:1935`), read by a
/// different method. Merging them would let a `/// <reference types="x" />`
/// answer an `import "x"` in the same file.
fn resolved_modules(
    requests: &[loader::ResolutionRequest],
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> FxHashMap<Path, FxHashMap<String, ModuleResolution>> {
    let mut by_file: FxHashMap<Path, FxHashMap<String, ModuleResolution>> = FxHashMap::default();
    for request in requests {
        if request.kind != loader::RequestKind::Module {
            continue;
        }
        let canonical =
            |name: &str| to_path(name, current_directory, use_case_sensitive_file_names);
        let answer = request
            .resolved
            .as_deref()
            .map_or(ModuleResolution::Unresolved, |name| ModuleResolution::To(canonical(name)));
        let containing = canonical(&request.containing_file);
        match by_file.entry(containing).or_default().entry(request.name.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(answer);
            }
            // The same specifier text, asked twice in one file. Upstream's key
            // carries the mode and keeps both; this key does not, so agreement
            // is the only case it can answer. Sticky: a third request that
            // happens to agree with the first must not clear a conflict the
            // second established.
            Entry::Occupied(mut slot) => {
                if *slot.get() != answer {
                    slot.insert(ModuleResolution::Conflicting);
                }
            }
        }
    }
    by_file
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program<'a>(arena: &'a Arena, files: &[(&str, &str)]) -> Program<'a> {
        Program::in_arena(
            arena,
            ProgramOptions {
                files: files
                    .iter()
                    .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
                    .collect(),
                ..Default::default()
            },
        )
    }

    #[test]
    fn a_program_holds_more_than_one_file_and_binds_them_all() {
        let arena = Arena::new();
        let program = program(&arena, &[("a.ts", "export const x = 1;"), ("b.ts", "const y = 2;")]);
        assert_eq!(program.source_files().len(), 2);
        assert_eq!(program.bound_file_count(), 2);
    }

    #[test]
    fn every_files_symbols_are_in_one_store_and_stay_distinguishable() {
        // Was `each_file_keeps_its_own_symbols`, and the rename is the change:
        // there is one store now, so the property worth asserting is that a
        // symbol is still attributable to the file that declared it — by the
        // node range its declaration falls in, which is the only thing that
        // answers "which text does this span index" once ids span the program.
        let arena = Arena::new();
        let program = program(&arena, &[("a.ts", "const x = 1;"), ("b.ts", "const y = 2;")]);
        let names: Vec<Vec<String>> = program
            .source_files()
            .iter()
            .map(|file| {
                let mut names: Vec<String> = program
                    .binder()
                    .symbols()
                    .iter()
                    .filter(|(_, s)| s.declarations.iter().any(|d| file.contains(*d)))
                    .map(|(_, s)| s.name.to_string())
                    .collect();
                names.sort();
                names
            })
            .collect();
        assert_eq!(names, vec![vec!["x".to_string()], vec!["y".to_string()]]);
    }

    #[test]
    fn a_file_is_reachable_by_either_spelling_of_its_path() {
        let arena = Arena::new();
        let program = program(&arena, &[("a.ts", "const x = 1;")]);
        assert!(program.source_file("a.ts").is_some());
        assert!(program.source_file("./a.ts").is_some());
        assert!(program.source_file("/a.ts").is_some());
        assert!(program.source_file("b.ts").is_none());
    }

    #[test]
    fn a_name_is_canonicalised_the_way_the_program_was_built_and_not_both_ways() {
        // `bd tsr-q89`. The lookup used to try the case-sensitive conversion and
        // then the case-insensitive one, which made a case-**sensitive** program
        // answer a name whose casing it does not hold — the fallback lowercases
        // `A.ts` to `/a.ts` and hits.
        //
        // Both directions on one program each, because a fix that simply dropped
        // the fallback would pass the first assertion and break the second: a
        // case-insensitive program genuinely must match `A.ts` to `a.ts`.
        let arena = Arena::new();
        let sensitive = Program::in_arena(
            &arena,
            ProgramOptions {
                files: vec![("a.ts".to_string(), "const x = 1;".to_string())],
                use_case_sensitive_file_names: true,
                ..Default::default()
            },
        );
        assert!(sensitive.source_file("a.ts").is_some(), "its own spelling");
        assert!(
            sensitive.source_file("A.ts").is_none(),
            "a case-sensitive program holds `a.ts` and `A.ts` is a different file"
        );

        let arena = Arena::new();
        let insensitive = Program::in_arena(
            &arena,
            ProgramOptions {
                files: vec![("a.ts".to_string(), "const x = 1;".to_string())],
                use_case_sensitive_file_names: false,
                ..Default::default()
            },
        );
        assert!(insensitive.source_file("a.ts").is_some());
        assert!(
            insensitive.source_file("A.ts").is_some(),
            "a case-insensitive program must still match the other casing"
        );
    }

    #[test]
    fn parsing_and_binding_are_separate_phases() {
        let arena = Arena::new();
        let mut program = Program::parse(
            &arena,
            ProgramOptions {
                files: vec![("a.ts".to_string(), "const x = 1;".to_string())],
                ..Default::default()
            },
        );
        assert_eq!(program.bound_file_count(), 0, "parse does not bind");
        assert_eq!(program.binder().symbols().len(), 0);
        program.bind_source_files(&arena);
        assert_eq!(program.bound_file_count(), 1);
        let after_one = program.binder().flow().len();
        assert!(after_one > 0);
        // Binding twice is a no-op, as upstream's `IsBound()` guard makes it.
        //
        // Asserted on the **flow-node count**, and that choice is the test. The
        // obvious assertion — the symbol count — cannot fail: re-binding a file
        // into a store that already holds it finds each name already in its
        // container's table and merges into the existing symbol, so a second
        // pass leaves the count alone whether or not the guard is there.
        // Verified by mutation: with the `bound_file_count` guard removed the
        // symbol count is unchanged and the flow count doubles. Flow nodes are
        // appended unconditionally, so they are the part of a bind that a
        // repeat cannot hide.
        program.bind_source_files(&arena);
        assert_eq!(program.binder().flow().len(), after_one, "a second bind adds nothing");
    }

    #[test]
    fn parse_diagnostics_are_reported_against_the_file_that_produced_them() {
        let arena = Arena::new();
        let program = program(&arena, &[("good.ts", "const x = 1;"), ("bad.ts", "const = ;")]);
        let diagnostics = program.syntactic_diagnostics();
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().all(|(path, _)| path.as_str().ends_with("bad.ts")));
    }

    #[test]
    fn a_dialect_follows_the_extension() {
        // `.tsx` reads a leading `<` as JSX and `.ts` does not; the program has
        // to decide per file, not per program.
        let arena = Arena::new();
        let program = program(&arena, &[("a.tsx", "const e = <X />;\ndeclare const X: any;")]);
        assert!(program.syntactic_diagnostics().is_empty());
    }

    #[test]
    fn the_options_reach_the_program() {
        let arena = Arena::new();
        let program = Program::in_arena(
            &arena,
            ProgramOptions {
                files: vec![("a.ts".to_string(), "const x = 1;".to_string())],
                compiler_options: CompilerOptions {
                    target: tsr_core::ScriptTarget::ES2020,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(program.compiler_options().emit_script_target(), tsr_core::ScriptTarget::ES2020);
    }

    // ---- A program discovered from its roots --------------------------------

    struct TestHost {
        fs: tsr_vfs::InMemoryFileSystem,
    }

    impl tsr_module::types::ResolutionHost for TestHost {
        fn fs(&self) -> &dyn tsr_vfs::FileSystem {
            &self.fs
        }

        fn current_directory(&self) -> &'static str {
            "/"
        }
    }

    fn host(files: &[(String, String)]) -> TestHost {
        TestHost { fs: tsr_vfs::InMemoryFileSystem::new(files.iter().cloned(), [], true) }
    }

    /// The bundled lib directory, or `None` when the submodule is not checked
    /// out. See docs/conventions.md — a submodule-dependent test skips.
    fn bundled_lib(name: &str) -> Option<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)?
            .join("vendor/typescript-go/internal/bundled/libs")
            .join(name);
        std::fs::read_to_string(path).ok()
    }

    #[test]
    fn a_program_discovers_the_files_its_roots_reach() {
        let files = [
            ("/a.ts".to_string(), "import { b } from \"./b\";\nexport const a = b;\n".to_string()),
            ("/b.ts".to_string(), "export const b = 1;\n".to_string()),
        ];
        let arena = Arena::new();
        let program = Program::from_root_files(
            &arena,
            &host(&files),
            LoadOptions { root_file_names: vec!["/a.ts".to_string()], ..Default::default() },
        );
        let mut names: Vec<&str> =
            program.source_files().iter().map(ProgramFile::file_name).collect();
        names.sort_unstable();
        assert_eq!(names, ["/a.ts", "/b.ts"], "`b.ts` was never named, only imported");
        assert_eq!(program.bound_file_count(), 2);
    }

    #[test]
    fn the_real_lib_file_is_loaded_bound_and_declares_the_globals() {
        // The end of the slice, asserted against the actual shipped file rather
        // than a fixture that says `declare interface Array<T> {}` — a fixture
        // would prove the loader can find a file it was told about and nothing
        // more. `Array`, `Object` and `String` are the three names the corpus
        // asks for most and all three are declared in `lib.es5.d.ts`.
        let Some(text) = bundled_lib("lib.es5.d.ts") else {
            return; // the submodule is not checked out
        };
        let files = [
            ("/a.ts".to_string(), "const x = 1;\n".to_string()),
            ("/libs/lib.es5.d.ts".to_string(), text),
        ];
        let arena = Arena::new();
        let program = Program::from_root_files(
            &arena,
            &host(&files),
            LoadOptions {
                compiler_options: CompilerOptions {
                    lib: vec!["es5".to_string()],
                    ..Default::default()
                },
                root_file_names: vec!["/a.ts".to_string()],
                default_library_path: "/libs".to_string(),
            },
        );

        assert_eq!(program.lib_files().len(), 1);
        assert_eq!(program.lib_files()[0].file_name(), "/libs/lib.es5.d.ts");
        assert_eq!(program.source_files()[0].file_name(), "/libs/lib.es5.d.ts", "libs lead");
        assert!(program.syntactic_diagnostics().is_empty(), "the shipped lib file must parse");

        let lib = &program.lib_files()[0];
        let declares = |name: &str| {
            program.binder().symbols().iter().any(|(_, symbol)| {
                symbol.name == name && symbol.declarations.iter().any(|d| lib.contains(*d))
            })
        };
        assert!(declares("Array"), "the whole point of loading a lib file");
        assert!(declares("Object"));
        assert!(declares("String"));
    }

    #[test]
    fn a_lib_files_symbols_are_the_whole_programs() {
        // **Inverted, not deleted.** This test was
        // `a_lib_files_symbols_are_still_the_lib_files_own` and asserted the
        // limit: a lib file's declarations were bound but invisible from every
        // other file, because a `SymbolId` and a `NodeId` meant one thing per
        // file. Program-wide identity is exactly the change that flips its
        // answer, so it is kept as the record of what moved — ADR-0034.
        let files = [
            ("/a.ts".to_string(), "const x = 1;\n".to_string()),
            ("/libs/lib.es5.d.ts".to_string(), "declare var globalThing: number;\n".to_string()),
        ];
        let arena = Arena::new();
        let program = Program::from_root_files(
            &arena,
            &host(&files),
            LoadOptions {
                compiler_options: CompilerOptions {
                    lib: vec!["es5".to_string()],
                    ..Default::default()
                },
                root_file_names: vec!["/a.ts".to_string()],
                default_library_path: "/libs".to_string(),
            },
        );
        let a = program.source_file("/a.ts").expect("the root file is in the program");
        let lib = &program.lib_files()[0];
        let resolve = |name: &str| {
            a.source_file().node_id.and_then(|id| {
                program.binder().resolve_name(
                    program.nodes(),
                    program.node_map(),
                    id,
                    name,
                    tsr_binder::SymbolFlags::VALUE,
                )
            })
        };
        // The positive control, kept from the original: without it this test
        // passes just as well when resolution answers `Some` for everything.
        assert!(resolve("x").is_some(), "a name in the file's own scope must resolve");
        let global = resolve("globalThing").expect("a lib file's global is now visible from /a.ts");

        // Not merely "something came back". The symbol has to be *the lib
        // file's own declaration*, which is the half that per-file identity
        // could not have delivered even with a name table: an id resolved
        // against the wrong node table would still have produced a symbol.
        let declarations = &program.binder().symbols().get(global).declarations;
        assert!(
            declarations.iter().all(|d| lib.contains(*d)),
            "the symbol's declarations must index the lib file's own range of the shared table"
        );
        assert!(
            declarations.iter().all(|d| !a.contains(*d)),
            "…and not /a.ts's, which is what reading the wrong file's declarations looks like"
        );
    }

    /// A file's `SourceFile` node id, which is how the checker names a file.
    fn file_id(program: &Program<'_>, name: &str) -> tsr_ast::NodeId {
        program
            .source_file(name)
            .expect("the file is in the program")
            .source_file()
            .node_id
            .expect("a parsed file's SourceFile is registered")
    }

    #[test]
    fn a_resolution_is_recalled_per_importing_file() {
        // The seam ADR-0041 exists for: the loader resolved `./b` to `/b.ts`
        // and dropped it, so a program knew which files it held and not which
        // import reached which file.
        let files = [
            ("/a.ts".to_string(), "import \"./b\";\nimport \"./nope\";\n".to_string()),
            ("/b.ts".to_string(), "export const b = 1;\n".to_string()),
        ];
        let arena = Arena::new();
        let program = Program::from_root_files(
            &arena,
            &host(&files),
            LoadOptions { root_file_names: vec!["/a.ts".to_string()], ..Default::default() },
        );
        let a = file_id(&program, "/a.ts");
        let b = file_id(&program, "/b.ts");

        assert_eq!(program.resolved_module(a, "./b"), Some(b));
        assert_eq!(
            program.resolved_module(a, "./nope"),
            None,
            "an unresolved import answers nothing, not the importing file"
        );
        // Pinned by construction, not by arithmetic: `/b.ts` contains no import
        // at all, so no code path could ever give it an entry. A map that is
        // per-program rather than per-importing-file answers `Some(b)` here and
        // every sum in the program still reconciles.
        assert_eq!(
            program.resolved_module(b, "./b"),
            None,
            "`/b.ts` never imported `./b`; only `/a.ts` did"
        );
    }

    #[test]
    fn two_modes_that_disagree_answer_nothing_rather_than_the_first() {
        // Under `module: node16` a static `import "./b"` resolves as CommonJS
        // and a dynamic `import("./b")` as ESNext (`getModeForUsageLocation`,
        // `internal/compiler/fileloader.go:726`), and only the first finds
        // `/b.ts`. Upstream keys its cache on the mode and keeps both answers;
        // this port cannot compute a usage location's mode, so the two collapse
        // onto one key — and when they disagree the honest answer is none.
        let node16 = || CompilerOptions {
            module: tsr_core::ModuleKind::Node16,
            module_resolution: tsr_core::ModuleResolutionKind::Node16,
            ..Default::default()
        };
        let both = [
            (
                "/a.ts".to_string(),
                "import \"./b\";\nexport const p = import(\"./b\");\n".to_string(),
            ),
            ("/b.ts".to_string(), "export const b = 1;\n".to_string()),
        ];
        let arena = Arena::new();
        let program = Program::from_root_files(
            &arena,
            &host(&both),
            LoadOptions {
                compiler_options: node16(),
                root_file_names: vec!["/a.ts".to_string()],
                ..Default::default()
            },
        );
        assert_eq!(
            program.resolved_module(file_id(&program, "/a.ts"), "./b"),
            None,
            "two modes, two answers, and no way to tell which usage is asking"
        );

        // The control, and it is the discriminating half: the *same specifier,
        // the same options, the same files*, with only the dynamic import
        // removed. If this also answered `None` the test above would be
        // measuring node16 resolution rather than the collapse.
        let static_only = [
            ("/a.ts".to_string(), "import \"./b\";\n".to_string()),
            ("/b.ts".to_string(), "export const b = 1;\n".to_string()),
        ];
        let arena = Arena::new();
        let program = Program::from_root_files(
            &arena,
            &host(&static_only),
            LoadOptions {
                compiler_options: node16(),
                root_file_names: vec!["/a.ts".to_string()],
                ..Default::default()
            },
        );
        assert_eq!(
            program.resolved_module(file_id(&program, "/a.ts"), "./b"),
            Some(file_id(&program, "/b.ts")),
            "one mode, one answer"
        );
    }

    #[test]
    fn a_program_built_from_a_file_list_resolves_no_module() {
        // `Program::new` runs no resolver, so it has no resolutions — a gap,
        // and deliberately not an attempt to re-derive them by matching the
        // specifier text against the file list.
        let arena = Arena::new();
        let program = program(&arena, &[("a.ts", "import \"./b\";"), ("b.ts", "export {};")]);
        assert_eq!(program.resolved_module(file_id(&program, "a.ts"), "./b"), None);
    }

    #[test]
    fn a_program_is_still_sendable_although_binding_is_no_longer_parallel() {
        // Renamed rather than deleted, because half of what it asserted stopped
        // being true and the other half became *more* load-bearing. Binding is
        // sequential now (one `SymbolStore`), so `Send` is no longer what makes
        // it parallel — but a corpus run still moves a whole case to a worker,
        // and that is what this holds in place.
        //
        // It survives the widening only because the arena is *borrowed*: a
        // `Program<'a>` holds `&'a` references into it, and `&T: Send` needs
        // `T: Sync`, which the AST is by ADR-0012's compile-time assertion. An
        // owning `Program` would not be `Send`, since the arena's bump pointer
        // is a `Cell`. That is the practical shape of ADR-0034's
        // caller-owns-the-arena decision: the arena is created inside the
        // worker, and the program is what crosses.
        const fn assert_send<T: Send>() {}
        assert_send::<ProgramFile<'_>>();
        assert_send::<Program<'_>>();
    }
}
