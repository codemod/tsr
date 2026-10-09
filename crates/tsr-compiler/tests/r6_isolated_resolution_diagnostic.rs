//! `resolveExternalModule`'s resolution-diagnostic branch
//! (`checker.go:15209`–`15260`): `GetResolutionDiagnostic`, TS2846 and
//! TS5097. `docs/parity/notes/r6-isolated.md` §5.

use tsr_checker::check::FileContext;
use tsr_compiler::{LoadOptions, Program};
use tsr_core::{CompilerOptions, ModuleKind, ModuleResolutionKind, ScriptTarget, Tristate};

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

/// The codes the checker reports in `file`, sorted.
fn codes(files: &[(&str, &str)], options: CompilerOptions, file: &str) -> Vec<u32> {
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
    let id = program.source_file(file).expect("loaded").source_file().node_id.expect("id");
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());
    checker.check_source_file(id, FileContext { ambient: false, has_parse_errors: false });
    let mut codes: Vec<u32> = checker
        .diagnostics()
        .iter()
        .filter(|(at, _)| *at == id)
        .map(|(_, diagnostic)| diagnostic.message.code())
        .collect();
    codes.sort_unstable();
    codes
}

fn options(isolated_modules: bool, verbatim_module_syntax: bool) -> CompilerOptions {
    CompilerOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        isolated_modules: Tristate::from_bool(isolated_modules),
        verbatim_module_syntax: Tristate::from_bool(verbatim_module_syntax),
        ..Default::default()
    }
}

fn bundler(
    allow_importing_ts_extensions: bool,
    allow_arbitrary_extensions: bool,
) -> CompilerOptions {
    CompilerOptions {
        module: ModuleKind::ESNext,
        module_resolution: ModuleResolutionKind::Bundler,
        allow_importing_ts_extensions: Tristate::from_bool(allow_importing_ts_extensions),
        allow_arbitrary_extensions: Tristate::from_bool(allow_arbitrary_extensions),
        no_emit: Tristate::True,
        ..options(false, false)
    }
}

/// `conformance/bundlerImportTsExtensions`, reduced.
const TS_EXTENSIONS: [(&str, &str); 4] = [
    ("/a.ts", "export {};\n"),
    ("/b.d.ts", "export {};\n"),
    ("/c.tsx", "export {};\n"),
    (
        "/main.ts",
        "import {} from './a';\nimport {} from './a.ts';\nimport {} from './b.d.ts';\n\
         import type {} from './b.d.ts';\nimport {} from './c.tsx';\n",
    ),
];

#[test]
fn a_ts_extension_needs_allow_importing_ts_extensions() {
    // TS5097 on `./a.ts` and `./c.tsx`, TS2846 on the non-type `./b.d.ts`,
    // TS6142 on the `.tsx` without `--jsx` even though it resolves to a
    // module. Native `tsgo` prints the same four on this program.
    assert_eq!(codes(&TS_EXTENSIONS, bundler(false, false), "/main.ts"), [2846, 5097, 5097, 6142]);
    assert_eq!(codes(&TS_EXTENSIONS, bundler(true, false), "/main.ts"), [2846, 6142]);
}

/// `conformance/declarationFileForHtmlImport`, reduced.
const HTML: [(&str, &str); 2] = [
    ("/component.d.html.ts", "export declare const blogPost: string;\n"),
    ("/file.ts", "import * as mod from './component.html';\nexport const x = mod;\n"),
];

#[test]
fn an_arbitrary_extension_needs_allow_arbitrary_extensions() {
    assert_eq!(codes(&HTML, bundler(false, false), "/file.ts"), [6263]);
    assert_eq!(codes(&HTML, bundler(false, true), "/file.ts"), [] as [u32; 0]);
}
