//! `checkAliasSymbol`'s `isolatedModules` / `verbatimModuleSyntax` arms
//! (`checker.go:6788`–`6858`, `tsr-checker/src/isolated_alias.rs`), over a
//! real program so imports resolve. `docs/parity/notes/r5-config.md` §6.
//!
//! The fixture is `compiler/isolatedModulesSketchyAliasLocalMerge`.

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

const FILES: [(&str, &str); 2] = [
    ("/types.ts", "export type FC = () => void;\n"),
    ("/bad.ts", "import { FC } from \"./types\";\nlet FC: FC | null = null;\n"),
];

/// The codes the checker reports in `/bad.ts`, sorted.
fn codes(isolated_modules: bool, verbatim_module_syntax: bool) -> Vec<u32> {
    let host = Host {
        fs: tsr_vfs::InMemoryFileSystem::new(
            FILES.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
            [],
            true,
        ),
    };
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: CompilerOptions {
                module: ModuleKind::CommonJS,
                target: ScriptTarget::ES2015,
                isolated_modules: Tristate::from_bool(isolated_modules),
                verbatim_module_syntax: Tristate::from_bool(verbatim_module_syntax),
                ..Default::default()
            },
            root_file_names: FILES.iter().map(|(name, _)| (*name).to_string()).collect(),
            ..Default::default()
        },
    );
    let bad = program.source_file("/bad.ts").expect("loaded").source_file().node_id.expect("id");
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());
    checker.check_source_file(bad, FileContext { ambient: false, has_parse_errors: false });
    let mut codes: Vec<u32> = checker
        .diagnostics()
        .iter()
        .filter(|(file, _)| *file == bad)
        .map(|(_, diagnostic)| diagnostic.message.code())
        .collect();
    codes.sort_unstable();
    codes
}

#[test]
fn isolated_modules_rejects_a_type_import_merged_with_a_local_value() {
    // TS2865: the import looks like a value to a single-file transpiler.
    assert_eq!(codes(true, false), [2865]);
}

#[test]
fn verbatim_module_syntax_requires_import_type_and_esm_in_commonjs_is_an_error() {
    // TS1484 (a type imported without `import type`) and TS1295 (ESM syntax
    // in a CommonJS-format file); TS2865 reads `isolatedModules` itself.
    assert_eq!(codes(false, true), [1295, 1484]);
    assert_eq!(codes(true, true), [1295, 1484, 2865]);
}

#[test]
fn neither_option_reports_nothing() {
    assert!(codes(false, false).is_empty());
}
