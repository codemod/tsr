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

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use tsr_core::CompilerOptions;
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
#[derive(Debug)]
pub struct Program {
    options: CompilerOptions,
    /// In the order they were given, which is the order diagnostics are
    /// reported in.
    files: Vec<ProgramFile>,
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
}

impl Program {
    /// Parse every file and bind it.
    ///
    /// Upstream splits these — `NewProgram` parses, `BindSourceFiles` binds on
    /// demand — and so does this, one line apart. They are separate because
    /// binding is the expensive half and a consumer that only wants syntax
    /// should not pay for it; [`Program::parse`] is that entry point.
    #[must_use]
    pub fn new(options: ProgramOptions) -> Self {
        let mut program = Self::parse(options);
        program.bind_source_files();
        program
    }

    /// Parse every file, without binding.
    #[must_use]
    pub fn parse(options: ProgramOptions) -> Self {
        let ProgramOptions {
            files,
            compiler_options,
            current_directory,
            use_case_sensitive_file_names,
        } = options;

        // One arena per file and no shared state, so this is a plain parallel
        // map — the same reason the conformance harness parallelises by case.
        let parsed: Vec<ProgramFile> = files
            .into_par_iter()
            .map(|(file_name, text)| {
                let path = to_path(&file_name, &current_directory, use_case_sensitive_file_names);
                ProgramFile::parse(path, file_name, text)
            })
            .collect();

        // First spelling wins, as upstream's `filesByPath` does: a file added
        // twice under two spellings of one path is one file, and the later one
        // is a redirect upstream tracks and we do not yet.
        let mut files_by_path = FxHashMap::default();
        for (index, file) in parsed.iter().enumerate() {
            files_by_path.entry(file.path().clone()).or_insert(index);
        }

        Self { options: compiler_options, files: parsed, files_by_path, lib_file_count: 0 }
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
        host: &dyn tsr_module::types::ResolutionHost,
        options: loader::LoadOptions,
    ) -> Self {
        let compiler_options = options.compiler_options.clone();
        let loaded = loader::FileLoader::load(host, options);

        let mut files_by_path = FxHashMap::default();
        for (index, file) in loaded.files.iter().enumerate() {
            files_by_path.entry(file.path().clone()).or_insert(index);
        }

        let mut program = Self {
            options: compiler_options,
            files: loaded.files,
            files_by_path,
            lib_file_count: loaded.lib_file_count,
        };
        program.bind_source_files();
        program
    }

    /// Bind every file that is not bound (`Program.BindSourceFiles`).
    ///
    /// Parallel across files, because each one is independent: a file's symbols
    /// depend only on that file. What crosses files — resolving an import to
    /// another file's exports — is the checker's, and happens after.
    pub fn bind_source_files(&mut self) {
        self.files.par_iter_mut().for_each(ProgramFile::bind);
    }

    /// The options every file is compiled under.
    #[must_use]
    pub fn compiler_options(&self) -> &CompilerOptions {
        &self.options
    }

    /// Every file, in the order given (`Program.SourceFiles`).
    ///
    /// Lib files lead, when there are any. See [`Program::lib_files`].
    #[must_use]
    pub fn source_files(&self) -> &[ProgramFile] {
        &self.files
    }

    /// The bundled `lib.*.d.ts` this program loaded, in load order.
    ///
    /// Load order is not incidental: a global interface declared in several libs
    /// merges in the order the files were added, so `lib.es5.d.ts`'s `Array`
    /// comes before `lib.es2015.iterable.d.ts`'s additions to it.
    #[must_use]
    pub fn lib_files(&self) -> &[ProgramFile] {
        &self.files[..self.lib_file_count]
    }

    /// Everything that is not a lib file, in the order the loader reached it.
    #[must_use]
    pub fn root_and_referenced_files(&self) -> &[ProgramFile] {
        &self.files[self.lib_file_count..]
    }

    /// The file with this canonical path (`Program.GetSourceFileByPath`).
    #[must_use]
    pub fn source_file_by_path(&self, path: &Path) -> Option<&ProgramFile> {
        self.files_by_path.get(path).map(|index| &self.files[*index])
    }

    /// The file this *file name* denotes (`Program.GetSourceFile`).
    ///
    /// Canonicalises the name first, so `./a.ts` and `/a.ts` find the same file.
    #[must_use]
    pub fn source_file(&self, file_name: &str, current_directory: &str) -> Option<&ProgramFile> {
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

    fn program(files: &[(&str, &str)]) -> Program {
        Program::new(ProgramOptions {
            files: files
                .iter()
                .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
                .collect(),
            ..Default::default()
        })
    }

    #[test]
    fn a_program_holds_more_than_one_file_and_binds_them_all() {
        let program = program(&[("a.ts", "export const x = 1;"), ("b.ts", "const y = 2;")]);
        assert_eq!(program.source_files().len(), 2);
        assert!(program.source_files().iter().all(ProgramFile::is_bound));
    }

    #[test]
    fn each_file_keeps_its_own_symbols() {
        // The point of the whole slice: two files, two symbol tables, one object
        // that can see both.
        let program = program(&[("a.ts", "const x = 1;"), ("b.ts", "const y = 2;")]);
        let names: Vec<Vec<String>> = program
            .source_files()
            .iter()
            .map(|file| {
                file.with_bound(|_, bound| {
                    let mut names: Vec<String> =
                        bound.symbols().iter().map(|(_, s)| s.name.to_string()).collect();
                    names.sort();
                    names
                })
            })
            .collect();
        assert_eq!(names, vec![vec!["x".to_string()], vec!["y".to_string()]]);
    }

    #[test]
    fn a_file_is_reachable_by_either_spelling_of_its_path() {
        let program = program(&[("a.ts", "const x = 1;")]);
        assert!(program.source_file("a.ts", "/").is_some());
        assert!(program.source_file("./a.ts", "/").is_some());
        assert!(program.source_file("/a.ts", "").is_some());
        assert!(program.source_file("b.ts", "/").is_none());
    }

    #[test]
    fn parsing_and_binding_are_separate_phases() {
        let mut program = Program::parse(ProgramOptions {
            files: vec![("a.ts".to_string(), "const x = 1;".to_string())],
            ..Default::default()
        });
        assert!(!program.source_files()[0].is_bound(), "parse does not bind");
        program.bind_source_files();
        assert!(program.source_files()[0].is_bound());
        // Binding twice is a no-op, as upstream's `IsBound()` guard makes it.
        program.bind_source_files();
        assert!(program.source_files()[0].is_bound());
    }

    #[test]
    fn parse_diagnostics_are_reported_against_the_file_that_produced_them() {
        let program = program(&[("good.ts", "const x = 1;"), ("bad.ts", "const = ;")]);
        let diagnostics = program.syntactic_diagnostics();
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().all(|(path, _)| path.as_str().ends_with("bad.ts")));
    }

    #[test]
    fn a_dialect_follows_the_extension() {
        // `.tsx` reads a leading `<` as JSX and `.ts` does not; the program has
        // to decide per file, not per program.
        let program = program(&[("a.tsx", "const e = <X />;\ndeclare const X: any;")]);
        assert!(program.syntactic_diagnostics().is_empty());
    }

    #[test]
    fn the_options_reach_the_program() {
        let program = Program::new(ProgramOptions {
            files: vec![("a.ts".to_string(), "const x = 1;".to_string())],
            compiler_options: CompilerOptions {
                target: tsr_core::ScriptTarget::ES2020,
                ..Default::default()
            },
            ..Default::default()
        });
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
        let program = Program::from_root_files(
            &host(&files),
            LoadOptions { root_file_names: vec!["/a.ts".to_string()], ..Default::default() },
        );
        let mut names: Vec<&str> =
            program.source_files().iter().map(ProgramFile::file_name).collect();
        names.sort_unstable();
        assert_eq!(names, ["/a.ts", "/b.ts"], "`b.ts` was never named, only imported");
        assert!(program.source_files().iter().all(ProgramFile::is_bound));
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
        let program = Program::from_root_files(
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

        let declares = |name: &str| {
            program.lib_files()[0].with_bound(|_, bound| {
                bound.symbols().iter().any(|(_, symbol)| symbol.name == name)
            })
        };
        assert!(declares("Array"), "the whole point of loading a lib file");
        assert!(declares("Object"));
        assert!(declares("String"));
    }

    #[test]
    fn a_lib_files_symbols_are_still_the_lib_files_own() {
        // The limit this slice stops at, asserted so it cannot be mistaken for a
        // capability: the program holds the lib file's symbols, and a *different*
        // file still cannot resolve a name into them. Nothing merges them into a
        // global scope, and the identity problem in the way is
        // docs/adr/0034-a-program-needs-one-identity-space.md.
        let files = [
            ("/a.ts".to_string(), "const x = 1;\n".to_string()),
            ("/libs/lib.es5.d.ts".to_string(), "declare var globalThing: number;\n".to_string()),
        ];
        let program = Program::from_root_files(
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
        let resolves = |name: &str| {
            a.with_bound_and_map(|source_file, bound, node_map| {
                source_file
                    .node_id
                    .and_then(|id| {
                        bound.resolve_name(
                            a.nodes(),
                            node_map,
                            id,
                            name,
                            tsr_binder::SymbolFlags::VALUE,
                        )
                    })
                    .is_some()
            })
        };
        // The positive control. Without it this test passes just as well when the
        // resolution call answers `None` for everything, and "cross-file
        // resolution does not work" is exactly what that looks like. Verified by
        // mutation: forcing `resolves` to `false` turns this line red.
        //
        // It does **not** guard the `meaning` argument. `BindResult::lookup_local`
        // is deliberately not meaning-filtered (see its docs), so passing
        // `SymbolFlags::empty()` here changes no answer — checked, and stated
        // rather than left as an implied guarantee.
        assert!(resolves("x"), "a name in the file's own scope must resolve");
        assert!(!resolves("globalThing"), "…and one in a lib file must not — see ADR-0034");
    }

    #[test]
    fn files_are_sendable_so_binding_can_be_parallel() {
        const fn assert_send<T: Send>() {}
        assert_send::<ProgramFile>();
        assert_send::<Program>();
    }
}
