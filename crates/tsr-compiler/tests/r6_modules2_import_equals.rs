//! r6-modules2's import-equals diff (`docs/parity/notes/r6-modules2.md` §1):
//! `import X = Y` resolves when `Y` is itself an import alias whose chain
//! carries the namespace meaning (`getSymbol`'s alias rule).
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

#[test]
fn export_import_of_an_imported_uninstantiated_namespace_is_ts1269() {
    // `compiler/isolatedModulesExportImportUninstantiatedNamespace`, plus a
    // local namespace; native `tsgo` reports both lines.
    let files = [
        ("/jsx.ts", "export namespace JSXInternal {\n  export type HTMLAttributes = string\n}\n"),
        (
            "/factory.ts",
            "import { JSXInternal } from \"./jsx\"\nexport import JSX = JSXInternal;\nnamespace Local { export type T = 1 }\nexport import L = Local;\n",
        ),
    ];
    let options = CompilerOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ESNext,
        isolated_modules: Tristate::True,
        ..Default::default()
    };
    assert_eq!(reported(&files, options, "/factory.ts"), [(2, 1269), (4, 1269)]);
}
