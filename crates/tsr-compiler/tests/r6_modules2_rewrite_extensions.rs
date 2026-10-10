//! r6-modules2's rewrite-extensions diff (`docs/parity/notes/r6-modules2.md`
//! §7): `resolveExternalModule`'s `rewriteRelativeImportExtensions` arm
//! (TS2876, TS2877), over a real program so specifiers resolve.
#![allow(
    clippy::cast_possible_truncation,
    reason = "line/column arithmetic over small test sources"
)]

use tsr_checker::check::FileContext;
use tsr_compiler::{LoadOptions, Program};
use tsr_core::{CompilerOptions, ModuleKind, ScriptTarget, Tristate};

struct Host {
    fs: tsr_vfs::InMemoryFileSystem,
}

impl tsr_module::types::ResolutionHost for Host {
    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        &self.fs
    }

    fn current_directory(&self) -> &'static str {
        "/"
    }
}

/// The `(line, code)` pairs the checker reports in `file`, sorted.
fn reported(files: &[(&str, &str)], options: CompilerOptions, file: &str) -> Vec<(u32, u32)> {
    let host = Host {
        fs: tsr_vfs::InMemoryFileSystem::new(
            files.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
            [],
            true,
        ),
    };
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: options,
            root_file_names: files.iter().map(|(name, _)| (*name).to_string()).collect(),
            ..Default::default()
        },
    );
    let text = files.iter().find(|(name, _)| *name == file).expect("listed").1;
    let id = program.source_file(file).expect("loaded").source_file().node_id.expect("id");
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());
    checker.check_source_file(id, FileContext { ambient: false, has_parse_errors: false });
    let mut reported: Vec<(u32, u32)> = checker
        .diagnostics()
        .iter()
        .filter(|(at, _)| *at == id)
        .map(|(_, diagnostic)| {
            let line = text[..diagnostic.span.start as usize].matches('\n').count() as u32 + 1;
            (line, diagnostic.message.code())
        })
        .collect();
    reported.sort_unstable();
    reported
}

fn options(rewrite: bool) -> CompilerOptions {
    CompilerOptions {
        module: ModuleKind::NodeNext,
        target: ScriptTarget::ES2022,
        rewrite_relative_import_extensions: Tristate::from_bool(rewrite),
        verbatim_module_syntax: Tristate::True,
        ..Default::default()
    }
}

#[test]
fn a_relative_ts_specifier_resolving_to_a_directory_is_ts2876() {
    // `conformance/rewriteRelativeImportExtensions/cjsErrors`: the type-only
    // import is exempt.
    let files = [
        ("/foo.ts/index.ts", "export = {};\n"),
        (
            "/index.ts",
            "import foo = require(\"./foo.ts\");\nimport type _foo = require(\"./foo.ts\");\n",
        ),
    ];
    assert_eq!(reported(&files, options(true), "/index.ts"), [(1, 2876)]);
    assert_eq!(reported(&files, options(false), "/index.ts"), []);
}

#[test]
fn a_non_relative_ts_specifier_to_an_emitted_file_is_ts2877() {
    // `conformance/rewriteRelativeImportExtensions/packageJsonImportsErrors`.
    let files = [
        (
            "/package.json",
            "{ \"name\": \"pkg\", \"type\": \"module\", \"imports\": { \"#foo.ts\": \"./foo.ts\", \"#internal/*\": \"./internal/*\" }, \"exports\": { \"./*.ts\": { \"source\": \"./*.ts\", \"default\": \"./*.js\" } } }\n",
        ),
        ("/foo.ts", "export {};\n"),
        ("/internal/foo.ts", "export {};\n"),
        (
            "/index.ts",
            "import {} from \"#foo.ts\";\nimport {} from \"#internal/foo.ts\";\nimport {} from \"pkg/foo.ts\";\n",
        ),
    ];
    assert_eq!(reported(&files, options(true), "/index.ts"), [(2, 2877)]);
    // Without the option the same import is TS5097, as native reports.
    assert_eq!(reported(&files, options(false), "/index.ts"), [(2, 5097)]);
}
