//! TS2866, `resolveNameHelper`'s isolatedModules success arm
//! (`checker.go:1872`–`1885`), over a real program so imports resolve.
//! `docs/parity/notes/r6-isolated.md` §2.

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

/// `compiler/isolatedModulesShadowGlobalTypeNotValue`'s `bad.ts`, reduced.
const FILES: [(&str, &str); 3] = [
    // The test program loads no lib, so the global value is declared here.
    ("/globals.d.ts", "declare var Date: { new (day: number): object };\n"),
    ("/types.ts", "export interface Date { day: number }\n"),
    (
        "/bad.ts",
        "import { Date } from './types';\nfunction foo(a: Date) { return new Date(a.day); }\n",
    ),
];

#[test]
fn a_type_import_shadowing_a_global_value_used_in_the_file_is_reported() {
    assert_eq!(codes(&FILES, options(true, false), "/bad.ts"), [2866]);
}

#[test]
fn verbatim_module_syntax_alone_does_not_report_ts2866() {
    // `compilerOptions.IsolatedModules`, not `GetIsolatedModules()`.
    assert!(!codes(&FILES, options(false, true), "/bad.ts").contains(&2866));
    assert_eq!(codes(&FILES, options(false, false), "/bad.ts"), [] as [u32; 0]);
}
