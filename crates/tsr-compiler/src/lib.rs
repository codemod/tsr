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
//! - **It does not resolve modules.** The file list is given, not discovered.
//!   Following `import` specifiers is the next slice, and it has its own oracle
//!   (146 `.trace.json` baselines).
//! - **It does not read tsconfig.json.** Options arrive as a
//!   [`CompilerOptions`], which is what upstream's `program.go` takes — it
//!   imports `internal/core` and not `internal/tsoptions`.
//! - **It does not merge globals.** Upstream does that in the *checker*
//!   (`initializeTypeChecker` merges each script file's locals into `c.globals`),
//!   and so should we, when there is one.

mod file;

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use tsr_core::CompilerOptions;
use tsr_path::{Path, to_path};

pub use file::ProgramFile;

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

        Self { options: compiler_options, files: parsed, files_by_path }
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
    #[must_use]
    pub fn source_files(&self) -> &[ProgramFile] {
        &self.files
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

    #[test]
    fn files_are_sendable_so_binding_can_be_parallel() {
        const fn assert_send<T: Send>() {}
        assert_send::<ProgramFile>();
        assert_send::<Program>();
    }
}
