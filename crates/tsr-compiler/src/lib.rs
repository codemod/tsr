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
//! - **It does not merge globals.** Upstream does that in the *checker*
//!   (`initializeTypeChecker` merges each script file's locals into `c.globals`),
//!   and so should we, when there is one. This is the reason a program that
//!   holds `lib.es5.d.ts` still cannot answer `Array`: see
//!   [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md),
//!   which says what is in the way and what would have to change.
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

use rustc_hash::FxHashMap;
use tsr_ast::{NodeMap, NodeTable};
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
    /// How many leading entries of `files` are bundled lib files.
    ///
    /// Upstream keeps `libFiles` as a separate slice and concatenates it in
    /// front (`filesparser.go:518`); the boundary is kept rather than the two
    /// slices, because everything else about a lib file is ordinary.
    lib_file_count: usize,
    /// Kind, span and parent for every node of **every** file
    /// ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)).
    nodes: NodeTable,
    /// The typed node behind each id, over the same numbering.
    node_map: NodeMap<'a>,
    /// Every symbol of every bound file, in one store.
    ///
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
        program.bind_source_files();
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
            parsed.push(ProgramFile::new(path, file_name, text, into));
        }

        // First spelling wins, as upstream's `filesByPath` does: a file added
        // twice under two spellings of one path is one file, and the later one
        // is a redirect upstream tracks and we do not yet.
        let mut files_by_path = FxHashMap::default();
        for (index, file) in parsed.iter().enumerate() {
            files_by_path.entry(file.path().clone()).or_insert(index);
        }

        Self {
            options: compiler_options,
            files: parsed,
            files_by_path,
            lib_file_count: 0,
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

        let mut program = Self {
            options: compiler_options,
            files: loaded.files,
            files_by_path,
            lib_file_count: loaded.lib_file_count,
            nodes: loaded.nodes,
            node_map: loaded.node_map,
            binder: BindResult::empty(),
            bound_file_count: 0,
        };
        program.bind_source_files();
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
    pub fn bind_source_files(&mut self) {
        for index in self.bound_file_count..self.files.len() {
            let file = &self.files[index];
            let previous = std::mem::replace(&mut self.binder, BindResult::empty());
            self.binder = tsr_binder::bind_into(
                previous,
                file.source_file(),
                &self.nodes,
                FileInfo { name: file.file_name(), text: file.text() },
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
    #[must_use]
    pub fn source_file(
        &self,
        file_name: &str,
        current_directory: &str,
    ) -> Option<&ProgramFile<'a>> {
        // The case sensitivity a program was built with is not stored, because
        // it is recoverable: a path that round-trips through the case-sensitive
        // conversion unchanged was built that way.
        for case_sensitive in [true, false] {
            let path = to_path(file_name, current_directory, case_sensitive);
            if let Some(file) = self.source_file_by_path(&path) {
                return Some(file);
            }
        }
        None
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
        assert!(program.source_file("a.ts", "/").is_some());
        assert!(program.source_file("./a.ts", "/").is_some());
        assert!(program.source_file("/a.ts", "").is_some());
        assert!(program.source_file("b.ts", "/").is_none());
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
        program.bind_source_files();
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
        program.bind_source_files();
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
        let a = program.source_file("/a.ts", "/").expect("the root file is in the program");
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
