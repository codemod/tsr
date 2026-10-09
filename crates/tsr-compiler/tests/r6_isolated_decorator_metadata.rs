//! `markDecoratorAliasReferenced` → `markEntityNameOrEntityExpressionAsReference`
//! (`checker.go:28686`, `:28857`): TS1272. `docs/parity/notes/r6-isolated.md` §3.

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

/// `compiler/emitDecoratorMetadata_isolatedModules`, reduced.
const FILES: [(&str, &str); 3] = [
    ("/type1.ts", "interface T1 {}\nexport type { T1 };\n"),
    ("/type2.ts", "export interface T2 {}\n"),
    (
        "/index.ts",
        "import { T1 } from './type1';\nimport type { T2 } from './type2';\n\
         declare var D: any;\nclass C {\n  @D m(a: T1, b: T2) {}\n}\nexport {};\n",
    ),
];

fn decorator_options(module: ModuleKind, emit_decorator_metadata: bool) -> CompilerOptions {
    CompilerOptions {
        module,
        experimental_decorators: Tristate::True,
        emit_decorator_metadata: Tristate::from_bool(emit_decorator_metadata),
        ..options(true, false)
    }
}

#[test]
fn a_type_only_value_in_decorated_metadata_needs_import_type() {
    // `T1` reaches only a type; `T2` is imported `import type`.
    assert_eq!(codes(&FILES, decorator_options(ModuleKind::ESNext, true), "/index.ts"), [1272]);
}

#[test]
fn ts1272_needs_emit_decorator_metadata_and_an_es_module_target() {
    assert_eq!(
        codes(&FILES, decorator_options(ModuleKind::ESNext, false), "/index.ts"),
        [] as [u32; 0]
    );
    assert_eq!(
        codes(&FILES, decorator_options(ModuleKind::CommonJS, true), "/index.ts"),
        [] as [u32; 0]
    );
}
