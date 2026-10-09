//! `checkConstEnumAccess` (`checker.go:7573`–`7597`): TS2475 and the
//! use-site TS2748. `docs/parity/notes/r6-isolated.md` §2.

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

#[test]
fn an_ambient_const_enum_access_is_an_error_under_isolated_modules() {
    // `compiler/isolatedModulesAmbientConstEnum`.
    let files = [("/file1.ts", "declare const enum E { X = 1 }\nexport var y = E.X;\n")];
    assert_eq!(codes(&files, options(true, false), "/file1.ts"), [2748]);
    assert_eq!(codes(&files, options(false, false), "/file1.ts"), [] as [u32; 0]);
}

#[test]
fn an_ambient_const_enum_reached_through_an_import_is_checked_at_the_import() {
    // `conformance/verbatimModuleSyntaxAmbientConstEnum`: the use of `E` goes
    // through an alias, so only `checkAliasSymbol` reports.
    let files = [
        ("/pkg.d.ts", "export declare const enum E { A, B }\n"),
        ("/a.ts", "import { E } from './pkg';\nexport const a = E.A;\n"),
    ];
    let esm = CompilerOptions { module: ModuleKind::ESNext, ..options(false, true) };
    assert_eq!(codes(&files, esm, "/a.ts"), [2748]);
}

#[test]
fn a_const_enum_object_outside_an_access_is_an_error() {
    // `compiler/constEnumErrors`: `foo(E2)`.
    let files =
        [("/a.ts", "const enum E2 { A }\nfunction foo(t: any): void {}\nfoo(E2);\nexport {};\n")];
    assert_eq!(codes(&files, options(false, false), "/a.ts"), [2475]);
}
