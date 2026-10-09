//! r6-isolated's ports over a real program, so imports resolve:
//! `checkExportAssignment`'s single-file-transpilation arms
//! (`checker.go:5609`–`5650`). `docs/parity/notes/r6-isolated.md`.

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

/// `conformance/exportDeclaration`: `export =` of a value exported type-only
/// from another file.
const EXPORT_EQUALS_TYPE_ONLY: [(&str, &str); 2] = [
    ("/a.ts", "class A {}\nexport type { A };\n"),
    ("/d.ts", "import { A } from './a';\nexport = A;\n"),
];

#[test]
fn export_equals_of_a_type_only_value_needs_import_type_under_isolated_modules() {
    // TS1289, and nothing without the option.
    assert_eq!(codes(&EXPORT_EQUALS_TYPE_ONLY, options(true, false), "/d.ts"), [1289]);
    assert_eq!(codes(&EXPORT_EQUALS_TYPE_ONLY, options(false, false), "/d.ts"), [] as [u32; 0]);
}

#[test]
fn export_default_of_an_imported_type_needs_export_type_under_isolated_modules() {
    // `compiler/isolatedModulesExportDeclarationType`'s test3: TS1292.
    let files = [
        ("/type.ts", "export type T = number;\n"),
        ("/test3.ts", "import { T } from \"./type\";\nexport default T;\n"),
    ];
    assert_eq!(codes(&files, options(true, false), "/test3.ts"), [1292]);
}

#[test]
fn export_default_of_a_type_is_an_error_under_verbatim_module_syntax() {
    // `conformance/verbatimModuleSyntaxNoElisionESM`'s main6: TS1284.
    let files = [("/main6.ts", "interface I {}\nexport default I;\n")];
    let esm = CompilerOptions { module: ModuleKind::ESNext, ..options(false, true) };
    assert_eq!(codes(&files, esm, "/main6.ts"), [1284]);
}
